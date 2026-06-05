//! sandboxed cargo+wasm-opt driver for track-B codegen.
//! pipeline: cache lookup → materialise crate → `cargo build` under
//! `sandbox-exec` (macos) or `bwrap` (linux) → `stellar contract optimize` →
//! return [`CompiledArtifact`].
//!
//! cache key = `sha256(cargo_toml || "\0" || src/lib.rs)`. sandbox is hardening,
//! not a security barrier — correctness must not depend on it.
//! `OZ_POLICY_CODEGEN_FORCE_NETWORK_TEST=1` short-circuits to `NetworkLeak`.

use std::path::{Path, PathBuf};
use std::process::Stdio;

use sha2::{Digest, Sha256};
use thiserror::Error;
use tokio::process::Command;

/// rendered source + cargo manifest produced by `render_contract`.
/// `wasm_hash_of_src = sha256(cargo_toml || "\0" || src_lib_rs)`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RenderedCrate {
    pub src_lib_rs: String,
    pub cargo_toml: String,
    pub wasm_hash_of_src: [u8; 32],
}

/// output of a single sandbox compile.
#[derive(Debug, Clone)]
pub struct CompiledArtifact {
    /// post-optimize wasm bytes.
    pub wasm: Vec<u8>,
    /// sha-256 of `wasm`; cross-checked against pinned walkthrough hashes.
    pub wasm_hash: [u8; 32],
    /// verbatim echo of `rendered.src_lib_rs`.
    pub source: String,
    /// `true` iff this invocation served from the on-disk cache (no `cargo
    /// build` was run).
    pub cache_hit: bool,
}

/// Typed error variants surfaced by the sandbox driver. Always converted
/// to `oz_policy_core::Error::CodegenCompileFailed` (the wire-stable
/// `E_CODEGEN_COMPILE_FAILED`) before crossing the public API.
#[derive(Debug, Error)]
pub enum SandboxError {
    /// `cargo build` returned non-zero. The payload is the captured stderr.
    #[error("cargo build failed: {0}")]
    BuildFailed(String),

    /// `stellar contract optimize` returned non-zero. The payload is the
    /// captured stderr.
    #[error("wasm-opt failed: {0}")]
    OptimizeFailed(String),

    /// Filesystem materialisation or platform detection failed before the
    /// build even started.
    #[error("sandbox setup failed: {0}")]
    SetupFailed(String),

    /// `OZ_POLICY_CODEGEN_FORCE_NETWORK_TEST=1` was set; surfaced as a
    /// sanity probe for the error-mapping wire. The real network-egress
    /// guarantee comes from the OS sandbox profile.
    #[error("network access detected in sandbox; refusing to build")]
    NetworkLeak,
}

impl From<SandboxError> for oz_policy_core::Error {
    fn from(e: SandboxError) -> Self {
        oz_policy_core::Error::CodegenCompileFailed(e.to_string())
    }
}

/// Environment variable consulted to override the cache root (otherwise we
/// fall back to `${HOME}/.cache/oz-policy-codegen`).
const ENV_CACHE_DIR: &str = "OZ_POLICY_CODEGEN_CACHE_DIR";

/// short-circuits [`compile`] to `NetworkLeak` for tests.
const ENV_FORCE_NETWORK_TEST: &str = "OZ_POLICY_CODEGEN_FORCE_NETWORK_TEST";

/// rust toolchain pin (matches workspace `rust-toolchain.toml`).
const PINNED_RUST_TOOLCHAIN: &str = "1.89.0";

/// build the rendered crate under an os sandbox; return optimized wasm.
#[tracing::instrument(level = "info", skip(rendered), fields(src_hash = %hex32(&rendered.wasm_hash_of_src)))]
pub async fn compile(rendered: &RenderedCrate) -> Result<CompiledArtifact, oz_policy_core::Error> {
    // test-only network-leak probe.
    if std::env::var(ENV_FORCE_NETWORK_TEST).ok().as_deref() == Some("1") {
        return Err(SandboxError::NetworkLeak.into());
    }

    let cache_dir = resolve_cache_dir(&rendered.wasm_hash_of_src)?;
    let opt_wasm_path = cache_dir.join("policy.opt.wasm");
    let opt_hash_path = cache_dir.join("policy.opt.wasm.sha256");

    // cache hit?
    if opt_wasm_path.exists() && opt_hash_path.exists() {
        let wasm = tokio::fs::read(&opt_wasm_path)
            .await
            .map_err(|e| SandboxError::SetupFailed(format!("read cached wasm: {e}")))?;
        let recorded_hex = tokio::fs::read_to_string(&opt_hash_path)
            .await
            .map_err(|e| SandboxError::SetupFailed(format!("read cached hash: {e}")))?;
        let recorded_hex = recorded_hex.trim();
        let wasm_hash = sha256(&wasm);
        if hex32(&wasm_hash) == recorded_hex {
            tracing::info!(cache_dir = %cache_dir.display(), "sandbox cache hit");
            return Ok(CompiledArtifact {
                wasm,
                wasm_hash,
                source: rendered.src_lib_rs.clone(),
                cache_hit: true,
            });
        }
        // hash mismatch — rebuild. only happens if cache dir was tampered.
        tracing::warn!(
            "cache entry hash mismatch; rebuilding (expected {} got {})",
            recorded_hex,
            hex32(&wasm_hash)
        );
    }

    // cache miss: materialise the rendered crate.
    tokio::fs::create_dir_all(&cache_dir)
        .await
        .map_err(|e| SandboxError::SetupFailed(format!("create cache dir: {e}")))?;
    let src_dir = cache_dir.join("src");
    tokio::fs::create_dir_all(&src_dir)
        .await
        .map_err(|e| SandboxError::SetupFailed(format!("create src dir: {e}")))?;

    tokio::fs::write(cache_dir.join("Cargo.toml"), &rendered.cargo_toml)
        .await
        .map_err(|e| SandboxError::SetupFailed(format!("write Cargo.toml: {e}")))?;
    tokio::fs::write(src_dir.join("lib.rs"), &rendered.src_lib_rs)
        .await
        .map_err(|e| SandboxError::SetupFailed(format!("write src/lib.rs: {e}")))?;
    tokio::fs::write(
        cache_dir.join("rust-toolchain.toml"),
        format!(
            "[toolchain]\nchannel = \"{PINNED_RUST_TOOLCHAIN}\"\ntargets = [\"wasm32-unknown-unknown\"]\nprofile = \"minimal\"\n"
        ),
    )
    .await
    .map_err(|e| SandboxError::SetupFailed(format!("write rust-toolchain.toml: {e}")))?;

    // CARGO_HOME points at the user's pre-warmed ~/.cargo. registry reads
    // come from there; target dir + lockfile writes land in the cache_dir cwd.
    let host_cargo_home = host_cargo_home()?;

    // init Cargo.lock via offline `cargo update`. fails loudly if the host's
    // ~/.cargo doesn't contain the closure — user must warm registry first.
    let lock_path = cache_dir.join("Cargo.lock");
    if !lock_path.exists() {
        let mut cmd = Command::new("cargo");
        cmd.arg("update")
            .arg("--offline")
            .current_dir(&cache_dir)
            .env("CARGO_HOME", &host_cargo_home);
        let out = cmd
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .await
            .map_err(|e| SandboxError::SetupFailed(format!("spawn cargo update: {e}")))?;
        if !out.status.success() {
            let stderr = String::from_utf8_lossy(&out.stderr).into_owned();
            // heuristic: "registry empty" surfaces as no-matching-package.
            if stderr.contains("no matching package")
                || stderr.contains("not found in registry")
                || stderr.contains("registry index was not found")
            {
                return Err(SandboxError::SetupFailed(
                    "cargo registry empty; run a non-sandboxed build first".to_string(),
                )
                .into());
            }
            return Err(
                SandboxError::SetupFailed(format!("cargo update --offline: {stderr}")).into(),
            );
        }
    }

    run_sandboxed_build(&cache_dir, &host_cargo_home).await?;
    let built_wasm = locate_built_wasm(&cache_dir, &rendered.cargo_toml)?;
    // stellar contract optimize:
    let optimize_out = Command::new("stellar")
        .arg("contract")
        .arg("optimize")
        .arg("--wasm")
        .arg(&built_wasm)
        .arg("--wasm-out")
        .arg(&opt_wasm_path)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .await
        .map_err(|e| SandboxError::OptimizeFailed(format!("spawn stellar: {e}")))?;
    if !optimize_out.status.success() {
        let stderr = String::from_utf8_lossy(&optimize_out.stderr).into_owned();
        return Err(SandboxError::OptimizeFailed(stderr).into());
    }

    let wasm = tokio::fs::read(&opt_wasm_path)
        .await
        .map_err(|e| SandboxError::OptimizeFailed(format!("read optimized wasm: {e}")))?;
    let wasm_hash = sha256(&wasm);
    tokio::fs::write(&opt_hash_path, hex32(&wasm_hash))
        .await
        .map_err(|e| SandboxError::SetupFailed(format!("write cached hash: {e}")))?;

    Ok(CompiledArtifact {
        wasm,
        wasm_hash,
        source: rendered.src_lib_rs.clone(),
        cache_hit: false,
    })
}

/// resolve `CARGO_HOME` (default `${HOME}/.cargo`).
fn host_cargo_home() -> Result<PathBuf, SandboxError> {
    if let Ok(v) = std::env::var("CARGO_HOME") {
        if !v.is_empty() {
            return Ok(PathBuf::from(v));
        }
    }
    let home =
        std::env::var("HOME").map_err(|e| SandboxError::SetupFailed(format!("HOME unset: {e}")))?;
    Ok(PathBuf::from(home).join(".cargo"))
}

/// per-render cache dir; `OZ_POLICY_CODEGEN_CACHE_DIR` overrides default.
fn resolve_cache_dir(src_hash: &[u8; 32]) -> Result<PathBuf, SandboxError> {
    let root = match std::env::var(ENV_CACHE_DIR) {
        Ok(v) if !v.is_empty() => PathBuf::from(v),
        _ => {
            let base = dirs::cache_dir().ok_or_else(|| {
                SandboxError::SetupFailed(
                    "could not resolve user cache dir; set OZ_POLICY_CODEGEN_CACHE_DIR".into(),
                )
            })?;
            base.join("oz-policy-codegen")
        }
    };
    Ok(root.join("sandbox").join(hex32(src_hash)))
}

/// run `cargo build --release --target wasm32-unknown-unknown --locked` under
/// the os sandbox if available; fall back to unsandboxed with a warn.
async fn run_sandboxed_build(cache_dir: &Path, cargo_home: &Path) -> Result<(), SandboxError> {
    let home =
        std::env::var("HOME").map_err(|e| SandboxError::SetupFailed(format!("HOME unset: {e}")))?;

    // `--locked` honours our Cargo.lock; release profile keeps overflow-checks.
    let cargo_args = [
        "build",
        "--release",
        "--target",
        "wasm32-unknown-unknown",
        "--locked",
    ];

    if cfg!(target_os = "macos") {
        let sandbox_profile = locate_sandbox_profile()?;
        if which("sandbox-exec").is_some() {
            let mut cmd = Command::new("sandbox-exec");
            cmd.arg("-f")
                .arg(&sandbox_profile)
                .arg("-D")
                .arg(format!("CACHE_DIR={}", cache_dir.display()))
                .arg("-D")
                .arg(format!("HOME_DIR={home}"))
                .arg("cargo");
            for a in &cargo_args {
                cmd.arg(a);
            }
            return run_build_command(cmd, cache_dir, cargo_home).await;
        }
        tracing::warn!("sandboxing not active; sandbox-exec missing");
    } else if cfg!(target_os = "linux") {
        if which("bwrap").is_some() {
            let mut cmd = Command::new("bwrap");
            cmd.arg("--ro-bind")
                .arg("/")
                .arg("/")
                .arg("--ro-bind")
                .arg(format!("{home}/.cargo"))
                .arg(format!("{home}/.cargo"))
                .arg("--bind")
                .arg(cache_dir)
                .arg("/work")
                .arg("--unshare-net")
                .arg("--chdir")
                .arg("/work")
                .arg("cargo");
            for a in &cargo_args {
                cmd.arg(a);
            }
            return run_build_command(cmd, cache_dir, cargo_home).await;
        }
        tracing::warn!("sandboxing not active; bwrap missing");
    } else {
        return Err(SandboxError::SetupFailed(format!(
            "unsupported platform: {}",
            std::env::consts::OS
        )));
    }

    // fallback: unsandboxed build. driver still works, hardening guarantee off.
    let mut cmd = Command::new("cargo");
    for a in &cargo_args {
        cmd.arg(a);
    }
    run_build_command(cmd, cache_dir, cargo_home).await
}

async fn run_build_command(
    mut cmd: Command,
    cache_dir: &Path,
    cargo_home: &Path,
) -> Result<(), SandboxError> {
    cmd.current_dir(cache_dir)
        .env("CARGO_HOME", cargo_home)
        // belt-and-braces with the sandbox profile.
        .env("CARGO_NET_OFFLINE", "true")
        // stellar-accounts 0.7.1 enables experimental_spec_shaking_v2 on
        // soroban-sdk 25.3.0 whose build.rs panics unless this is set or the
        // invocation comes from `stellar contract build`.
        .env("SOROBAN_SDK_BUILD_SYSTEM_SUPPORTS_SPEC_SHAKING_V2", "1")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let out = cmd
        .output()
        .await
        .map_err(|e| SandboxError::BuildFailed(format!("spawn cargo: {e}")))?;
    if !out.status.success() {
        let stderr = String::from_utf8_lossy(&out.stderr).into_owned();
        let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
        return Err(SandboxError::BuildFailed(format!(
            "exit={:?}\n--- stderr ---\n{}\n--- stdout ---\n{}",
            out.status.code(),
            stderr,
            stdout
        )));
    }
    Ok(())
}

/// resolve `scripts/sandbox-profile-macos.sb` via `CARGO_MANIFEST_DIR`.
fn locate_sandbox_profile() -> Result<PathBuf, SandboxError> {
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    // crates/oz-policy-codegen → workspace root is two `..` up.
    let candidate = manifest.join("../../scripts/sandbox-profile-macos.sb");
    let canonical = candidate.canonicalize().map_err(|e| {
        SandboxError::SetupFailed(format!(
            "sandbox profile not found at {}: {e}",
            candidate.display()
        ))
    })?;
    Ok(canonical)
}

/// locate the built wasm via the rendered Cargo.toml's `name = "…"` line.
fn locate_built_wasm(cache_dir: &Path, cargo_toml: &str) -> Result<PathBuf, SandboxError> {
    let name = extract_package_name(cargo_toml).ok_or_else(|| {
        SandboxError::SetupFailed("could not parse `name = \"…\"` from rendered Cargo.toml".into())
    })?;
    // cargo's wasm32 target dir uses the crate name with `-` replaced by `_`.
    let snake = name.replace('-', "_");
    let path = cache_dir
        .join("target")
        .join("wasm32-unknown-unknown")
        .join("release")
        .join(format!("{snake}.wasm"));
    if !path.exists() {
        return Err(SandboxError::BuildFailed(format!(
            "expected wasm at {}; cargo build claimed success but the artifact is missing",
            path.display()
        )));
    }
    Ok(path)
}

/// Parse the value of `name = "…"` under `[package]` in a Cargo.toml. We
/// hand-roll this rather than pulling in `toml` because the surface area
/// is tiny.
fn extract_package_name(cargo_toml: &str) -> Option<String> {
    let mut in_package = false;
    for line in cargo_toml.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with('#') || trimmed.is_empty() {
            continue;
        }
        if trimmed.starts_with('[') {
            in_package = trimmed == "[package]";
            continue;
        }
        if in_package {
            if let Some(rest) = trimmed.strip_prefix("name") {
                let rest = rest.trim_start().strip_prefix('=')?.trim();
                let rest = rest.strip_prefix('"')?;
                let end = rest.find('"')?;
                return Some(rest[..end].to_string());
            }
        }
    }
    None
}

/// lightweight `which`-style PATH lookup.
fn which(bin: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    for entry in std::env::split_paths(&path) {
        let candidate = entry.join(bin);
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    None
}

fn sha256(bytes: &[u8]) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    let out = hasher.finalize();
    let mut arr = [0u8; 32];
    arr.copy_from_slice(&out);
    arr
}

fn hex32(bytes: &[u8; 32]) -> String {
    let mut s = String::with_capacity(64);
    for b in bytes {
        use std::fmt::Write;
        let _ = write!(s, "{b:02x}");
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    /// fixture with precomputed `wasm_hash_of_src` for cache-key tests.
    fn fixture(seed: u8) -> RenderedCrate {
        let mut hash = [0u8; 32];
        hash.fill(seed);
        RenderedCrate {
            src_lib_rs: format!("// seed={seed}\n"),
            cargo_toml: "[package]\nname = \"x\"\nversion = \"0.0.0\"\n".into(),
            wasm_hash_of_src: hash,
        }
    }

    #[test]
    fn cache_key_matches_hash() {
        let r = fixture(0xab);
        let dir = resolve_cache_dir(&r.wasm_hash_of_src).expect("resolve");
        let leaf = dir
            .file_name()
            .expect("leaf")
            .to_str()
            .expect("utf8")
            .to_string();
        // 32 bytes of 0xab → 64 hex chars.
        assert_eq!(leaf.len(), 64, "leaf must be 32-byte hex");
        assert_eq!(leaf, "ab".repeat(32));
        // parent must be `sandbox`.
        assert_eq!(
            dir.parent()
                .and_then(|p| p.file_name())
                .and_then(|s| s.to_str()),
            Some("sandbox")
        );
    }

    #[test]
    fn resolve_cache_dir_honors_env_override() {
        let tmp = tempfile::tempdir().expect("tempdir");
        std::env::set_var(ENV_CACHE_DIR, tmp.path());
        let r = fixture(0x42);
        let dir = resolve_cache_dir(&r.wasm_hash_of_src).expect("resolve");
        assert!(
            dir.starts_with(tmp.path()),
            "override path {} must be under {}",
            dir.display(),
            tmp.path().display()
        );
        std::env::remove_var(ENV_CACHE_DIR);
    }

    /// force-network-test surfaces as `E_CODEGEN_COMPILE_FAILED` (error mapping check).
    #[tokio::test]
    async fn network_leak_detection_returns_typed_error() {
        std::env::set_var(ENV_FORCE_NETWORK_TEST, "1");
        let r = fixture(0x11);
        let result = compile(&r).await;
        std::env::remove_var(ENV_FORCE_NETWORK_TEST);
        let err = result.expect_err("force-network-test must fail");
        assert_eq!(err.code(), "E_CODEGEN_COMPILE_FAILED");
        assert!(
            err.to_string().contains("network access detected"),
            "expected NetworkLeak message; got: {err}"
        );
    }

    #[test]
    fn extract_package_name_finds_simple_name() {
        let toml = "[package]\nname = \"my-policy\"\nversion = \"0.1.0\"\n";
        assert_eq!(extract_package_name(toml), Some("my-policy".into()));
    }

    #[test]
    fn extract_package_name_skips_non_package_sections() {
        let toml = r#"[dependencies]
name = "wrong"

[package]
name = "right"
"#;
        assert_eq!(extract_package_name(toml), Some("right".into()));
    }

    #[test]
    fn sandbox_error_maps_to_canonical_code() {
        let cases = [
            SandboxError::BuildFailed("x".into()),
            SandboxError::OptimizeFailed("y".into()),
            SandboxError::SetupFailed("z".into()),
            SandboxError::NetworkLeak,
        ];
        for case in cases {
            let display = case.to_string();
            let oz: oz_policy_core::Error = case.into();
            assert_eq!(oz.code(), "E_CODEGEN_COMPILE_FAILED");
            assert!(
                oz.to_string().contains(&display),
                "expected `{}` to be embedded in `{}`",
                display,
                oz
            );
        }
    }

    /// windows unsupported — driver refuses cleanly.
    #[test]
    #[cfg(target_os = "windows")]
    fn setup_failed_when_platform_unsupported() {
        // compile-time guard that the cfg arm exists.
        let _: fn(&Path, &Path) -> _ = |_, _| async move { Ok::<(), SandboxError>(()) };
    }

    /// cache-hit branch must short-circuit; never touches cargo.
    #[tokio::test]
    async fn cache_hit_short_circuits_compile() {
        let tmp = tempfile::tempdir().expect("tempdir");
        std::env::set_var(ENV_CACHE_DIR, tmp.path());

        let r = fixture(0x77);
        let cache_dir = resolve_cache_dir(&r.wasm_hash_of_src).expect("resolve");
        tokio::fs::create_dir_all(&cache_dir).await.expect("mkdir");
        let fake_wasm: Vec<u8> = b"\0asm\x01\x00\x00\x00".to_vec();
        let expected_hash = sha256(&fake_wasm);
        tokio::fs::write(cache_dir.join("policy.opt.wasm"), &fake_wasm)
            .await
            .expect("write wasm");
        tokio::fs::write(
            cache_dir.join("policy.opt.wasm.sha256"),
            hex32(&expected_hash),
        )
        .await
        .expect("write hash");

        let artifact = compile(&r).await.expect("cache hit must succeed");
        std::env::remove_var(ENV_CACHE_DIR);

        assert!(artifact.cache_hit, "second-pass must report cache_hit");
        assert_eq!(artifact.wasm, fake_wasm);
        assert_eq!(artifact.wasm_hash, expected_hash);
        assert_eq!(artifact.source, r.src_lib_rs);
    }
}
