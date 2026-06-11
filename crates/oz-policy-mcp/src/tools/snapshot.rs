//! `create_snapshot` / `get_snapshot` MCP tools + on-disk snapshot store
//! + background GC.
//!
//! per playground design §3.4 (snapshot store) + §6.5 (id unguessability).
//!
//! ## On-disk layout
//!
//! one JSON file per snapshot at `<dir>/<id>.json`. The directory is
//! resolved via the env var `OZ_POLICY_SNAPSHOT_DIR` first (so integration
//! tests can scope writes to a `tempfile::tempdir()`), falling back to
//! `/var/lib/oz-policy-mcp/snapshots/` (writable via the existing
//! `StateDirectory=oz-policy-mcp` systemd unit — see spec §10). The
//! directory is created on first use if missing.
//!
//! ## ID encoding
//!
//! Crockford base32 (alphabet `0123456789ABCDEFGHJKMNPQRSTVWXYZ`, no
//! `I` / `L` / `O` / `U`) of the low 40 bits of `rand::rng().random::<u64>()`,
//! produced as an 8-char ASCII string. 40 bits of randomness against a
//! 30-day retention window is collision-resistant at the scale this builder
//! ships (see spec §6.5 for the trade-off rationale).
//!
//! On disk-exists collision, we retry up to 5 times before surfacing an
//! `internal_error`. Five retries × 40 bits → effectively
//! ~5×2⁻⁴⁰ ≈ 4.5e-12 chance of failure even with millions of live snapshots.
//!
//! ## Retention + GC
//!
//! every snapshot's `expires_at = created_at + 30 days`. A background
//! tokio task ([`spawn_gc`]) wakes every 6 hours, walks the store directory,
//! parses each file's `expires_at`, and unlinks the expired ones. Failures
//! (read, parse, unlink) get logged via `tracing::warn!` and the loop
//! continues — GC is best-effort.
//!
//! ## `get_snapshot` error mapping
//!
//! every miss (file not found, id format invalid, expires_at in the past)
//! surfaces as `E_SNAPSHOT_NOT_FOUND`. Path-style errors (permission
//! denied, etc.) are flattened into the same code so the wire surface does
//! not leak filesystem details to MCP clients.

use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use std::time::Duration;

use chrono::{DateTime, Utc};
use oz_policy_core::{recording::Recording, spec::PolicySpec, Error};
use oz_policy_simhost::run::SimReport;
use rand::Rng;
use regex::Regex;
use rmcp::model::ErrorData;
use serde::{Deserialize, Serialize};

use crate::error_mapping::error_to_jsonrpc;
use crate::store::McpStore;

/// canonical fallback snapshot directory used in production. Tests override
/// via the `OZ_POLICY_SNAPSHOT_DIR` env var.
const DEFAULT_SNAPSHOT_DIR: &str = "/var/lib/oz-policy-mcp/snapshots";

/// env var that overrides [`DEFAULT_SNAPSHOT_DIR`]. Set by integration tests
/// to a `tempfile::tempdir()` path.
pub const SNAPSHOT_DIR_ENV: &str = "OZ_POLICY_SNAPSHOT_DIR";

/// retention window for newly-created snapshots, in days. After
/// `created_at + RETENTION_DAYS days`, [`get_snapshot`] returns
/// `E_SNAPSHOT_NOT_FOUND` and the GC task unlinks the file on its next
/// pass. Kept as an integer constant (rather than a `chrono::Duration`)
/// because `Duration::days` is not yet `const` in chrono 0.4.x.
pub const RETENTION_DAYS: i64 = 30;

/// retention window as a `chrono::Duration`. Built once via [`OnceLock`]
/// since `Duration::days` isn't `const` in chrono 0.4.x.
pub fn retention() -> chrono::Duration {
    chrono::Duration::days(RETENTION_DAYS)
}

/// GC interval — the background task wakes this often and walks the store
/// directory. 6 hours is per spec §3.4; tests use a shorter interval via
/// [`spawn_gc_with_interval`].
pub const GC_INTERVAL: Duration = Duration::from_secs(6 * 60 * 60);

/// crockford base32 alphabet (no `I`, `L`, `O`, `U`). 32 chars.
const CROCKFORD_ALPHABET: &[u8; 32] = b"0123456789ABCDEFGHJKMNPQRSTVWXYZ";

/// length of generated snapshot ids — 8 chars × 5 bits = 40 bits.
const SNAPSHOT_ID_LEN: usize = 8;

/// max disk-exists retries before surfacing an internal_error.
const MAX_ID_GEN_RETRIES: usize = 5;

/// returns the snapshot directory in use. Reads the env var first (test
/// override path) and falls back to the production default. The returned
/// path is created on disk if it does not exist yet.
pub fn snapshot_dir() -> PathBuf {
    let dir = std::env::var(SNAPSHOT_DIR_ENV)
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from(DEFAULT_SNAPSHOT_DIR));
    if !dir.exists() {
        if let Err(e) = std::fs::create_dir_all(&dir) {
            tracing::warn!(
                error = ?e,
                dir = %dir.display(),
                "failed to create snapshot directory; subsequent writes will surface the error"
            );
        }
    }
    dir
}

/// id-format validator (regex). Compiled once via [`OnceLock`].
fn id_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    // Crockford alphabet without I/L/O/U: 0-9 A-H J K M N P-T V-Z.
    RE.get_or_init(|| Regex::new(r"^[0-9A-HJKMNP-TV-Z]{8}$").expect("snapshot id regex is valid"))
}

/// produces a fresh 8-char Crockford base32 id from the low 40 bits of a
/// `u64` random draw. Pure function — separated from the
/// retry-on-disk-collision loop so unit tests can drive it deterministically.
fn id_from_u64(bits: u64) -> String {
    let mut out = [0u8; SNAPSHOT_ID_LEN];
    // take the low 40 bits, emit 8 base32 chars MSB-first.
    let mut v = bits & 0xFF_FFFF_FFFF;
    for i in (0..SNAPSHOT_ID_LEN).rev() {
        let idx = (v & 0x1F) as usize;
        out[i] = CROCKFORD_ALPHABET[idx];
        v >>= 5;
    }
    // SAFETY: every byte we wrote is one of the Crockford alphabet chars,
    // all of which are valid ASCII (and therefore valid UTF-8).
    String::from_utf8(out.to_vec()).expect("Crockford alphabet is ASCII")
}

/// generate a fresh snapshot id that does not collide with an existing file
/// in `dir`. Retries up to [`MAX_ID_GEN_RETRIES`] times before surfacing an
/// `internal_error`. The 5-retry bound is per spec §3.4.
fn fresh_id_in_dir(dir: &Path) -> Result<String, ErrorData> {
    for _ in 0..MAX_ID_GEN_RETRIES {
        let bits = rand::rng().random::<u64>();
        let id = id_from_u64(bits);
        let path = dir.join(format!("{id}.json"));
        if !path.exists() {
            return Ok(id);
        }
    }
    Err(ErrorData::internal_error(
        "create_snapshot: exhausted snapshot id retries (5 collisions on disk)",
        None,
    ))
}

// on-disk record

/// the complete on-disk snapshot record. Persisted as a single JSON file
/// under [`snapshot_dir`]. Fields are stable wire surface — additions go
/// at the end with a `#[serde(default)]` so old snapshots remain readable.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct SnapshotRecord {
    /// 8-char Crockford base32 id (matches the filename stem).
    pub snapshot_id: String,
    /// RFC3339 / ISO8601 UTC timestamp at create time.
    pub created_at: DateTime<Utc>,
    /// `created_at + 30 days`. Past-`expires_at` snapshots surface as
    /// `E_SNAPSHOT_NOT_FOUND` even if the file still exists on disk (GC
    /// catches up on its next pass).
    pub expires_at: DateTime<Utc>,
    /// frozen copy of the source recording (by value — the originating
    /// `recording_id` may have been GC'd from the recorder cache).
    pub recording: Recording,
    /// frozen copy of the source spec.
    pub spec: PolicySpec,
    /// user-edited `lib.rs` body when the snapshot represents a custom-
    /// source pipeline. `None` for snapshots taken straight from the
    /// synthesised pipeline.
    pub modified_lib_rs: Option<String>,
    /// frozen `SimReport` shown at snapshot time.
    pub report: SimReport,
}

// create_snapshot

/// `create_snapshot` input. Resolves `recording_id` + `spec_id` through
/// the in-memory [`McpStore`] and freezes the resolved payloads INTO the
/// on-disk snapshot so downstream `get_snapshot` calls survive after the
/// originating cache entries are GC'd.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CreateSnapshotInput {
    /// `rec_<uuid>` id from an earlier `record_transaction` call.
    pub recording_id: String,
    /// `spec_<uuid>` id from an earlier `synthesize_policy` call.
    pub spec_id: String,
    /// optional user-edited `lib.rs` from a custom-source pipeline.
    pub modified_lib_rs: Option<String>,
    /// the `SimReport` the user is sharing.
    pub report: SimReport,
}

/// `create_snapshot` output — minimal reference handed back to the caller
/// so the frontend can build the `/playground/s/<id>` URL.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct CreateSnapshotOutput {
    /// 8-char Crockford base32 id; embedded in the share URL.
    pub snapshot_id: String,
    /// RFC3339 UTC; frontend renders this in the share toast.
    pub expires_at: DateTime<Utc>,
}

/// `create_snapshot` handler. Pipeline:
///
/// 1. Resolve `recording_id` + `spec_id` through the store. A miss surfaces
///    as `invalid_params` (-32602), matching the other tool handlers'
///    convention (the store layer has no `E_RECORDER_NOT_FOUND` /
///    `E_SPEC_NOT_FOUND` codes for in-memory misses — only on-disk misses
///    of `get_snapshot` use the `E_SNAPSHOT_NOT_FOUND` code).
/// 2. Generate a fresh 8-char Crockford base32 id (collision-retry up to 5
///    times against on-disk filenames).
/// 3. Write the full record (recording + spec by value, plus optional
///    `modified_lib_rs`, plus the `SimReport`) to `<dir>/<id>.json`.
/// 4. Return `{ snapshot_id, expires_at }`.
pub async fn create_snapshot(
    store: &McpStore,
    input: CreateSnapshotInput,
) -> Result<CreateSnapshotOutput, ErrorData> {
    let recording = store.get_recording(&input.recording_id).ok_or_else(|| {
        ErrorData::invalid_params(
            format!(
                "create_snapshot: recording_id {:?} not found in store",
                input.recording_id
            ),
            None,
        )
    })?;
    let spec = store.get_spec(&input.spec_id).ok_or_else(|| {
        ErrorData::invalid_params(
            format!(
                "create_snapshot: spec_id {:?} not found in store",
                input.spec_id
            ),
            None,
        )
    })?;

    let dir = snapshot_dir();
    let snapshot_id = fresh_id_in_dir(&dir)?;
    let created_at = Utc::now();
    let expires_at = created_at + retention();

    let record = SnapshotRecord {
        snapshot_id: snapshot_id.clone(),
        created_at,
        expires_at,
        recording,
        spec,
        modified_lib_rs: input.modified_lib_rs,
        report: input.report,
    };

    let path = dir.join(format!("{snapshot_id}.json"));
    let bytes = serde_json::to_vec(&record).map_err(|e| {
        ErrorData::internal_error(
            format!("create_snapshot: serialize snapshot record: {e}"),
            None,
        )
    })?;
    // write-then-rename gives atomic visibility — a concurrent GC pass
    // never observes a half-written `<id>.json`. (`tempfile` is a
    // dev-dependency only; use a manual `.<id>.json.tmp` sibling.)
    let tmp_path = dir.join(format!(".{snapshot_id}.json.tmp"));
    std::fs::write(&tmp_path, &bytes).map_err(|e| {
        ErrorData::internal_error(
            format!(
                "create_snapshot: write snapshot tmp file {}: {e}",
                tmp_path.display()
            ),
            None,
        )
    })?;
    std::fs::rename(&tmp_path, &path).map_err(|e| {
        // cleanup the tmp; if cleanup itself fails the next GC pass will
        // skip it (it doesn't match `<id>.json`).
        let _ = std::fs::remove_file(&tmp_path);
        ErrorData::internal_error(
            format!(
                "create_snapshot: rename snapshot file {} -> {}: {e}",
                tmp_path.display(),
                path.display()
            ),
            None,
        )
    })?;

    Ok(CreateSnapshotOutput {
        snapshot_id,
        expires_at,
    })
}

// get_snapshot

/// `get_snapshot` input — bare id lookup.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct GetSnapshotInput {
    /// 8-char Crockford base32 id from a prior `create_snapshot`.
    pub snapshot_id: String,
}

/// `get_snapshot` handler. Validates the id format first (so an invalid id
/// never reaches the filesystem layer — the regex narrows the input enough
/// that path-traversal attacks like `../../etc/passwd` are rejected before
/// any disk touch).
///
/// errors:
/// * `E_SNAPSHOT_NOT_FOUND` — id format invalid, file missing, OR file
///   present but `expires_at` in the past. Flattening these into a single
///   code keeps the wire surface from leaking whether a given id has ever
///   been used (see spec §7's "errors are surfaced as `error_code` + human
///   message").
pub async fn get_snapshot(input: GetSnapshotInput) -> Result<SnapshotRecord, ErrorData> {
    let id = input.snapshot_id;
    if !id_regex().is_match(&id) {
        return Err(error_to_jsonrpc(&Error::SnapshotNotFound(format!(
            "snapshot_id {id:?} has invalid format"
        ))));
    }

    let dir = snapshot_dir();
    let path = dir.join(format!("{id}.json"));
    let bytes = match std::fs::read(&path) {
        Ok(b) => b,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return Err(error_to_jsonrpc(&Error::SnapshotNotFound(format!(
                "snapshot_id {id:?} not found"
            ))));
        }
        Err(e) => {
            // we deliberately collapse permission / I/O errors into the
            // same E_SNAPSHOT_NOT_FOUND surface to avoid leaking
            // filesystem state to MCP clients. Log the underlying error
            // so an operator can still triage.
            tracing::warn!(error = ?e, path = %path.display(), "get_snapshot: read error mapped to E_SNAPSHOT_NOT_FOUND");
            return Err(error_to_jsonrpc(&Error::SnapshotNotFound(format!(
                "snapshot_id {id:?} not found"
            ))));
        }
    };

    let record: SnapshotRecord = match serde_json::from_slice(&bytes) {
        Ok(r) => r,
        Err(e) => {
            tracing::warn!(error = ?e, path = %path.display(), "get_snapshot: parse error mapped to E_SNAPSHOT_NOT_FOUND");
            return Err(error_to_jsonrpc(&Error::SnapshotNotFound(format!(
                "snapshot_id {id:?} not found"
            ))));
        }
    };

    if record.expires_at <= Utc::now() {
        return Err(error_to_jsonrpc(&Error::SnapshotNotFound(format!(
            "snapshot_id {id:?} expired"
        ))));
    }

    Ok(record)
}

// gC

/// spawn the background GC task on the global tokio runtime. Wakes every
/// [`GC_INTERVAL`] (6 hours), iterates the snapshot directory, and unlinks
/// any file whose `expires_at` is in the past. Failures (read error, parse
/// error, unlink error) get logged via `tracing::warn!` and the loop
/// continues — GC is best-effort by spec §3.4 ("GC: background tokio task
/// ... deletes files whose `expires_at` is in the past").
///
/// the returned [`tokio::task::JoinHandle`] is detached by `main.rs`;
/// tests use [`spawn_gc_with_interval`] to drive shorter cycles.
pub fn spawn_gc() -> tokio::task::JoinHandle<()> {
    spawn_gc_with_interval(GC_INTERVAL)
}

/// test-friendly GC spawner. Same semantics as [`spawn_gc`] but with a
/// caller-controlled tick interval.
pub fn spawn_gc_with_interval(interval: Duration) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let mut ticker = tokio::time::interval(interval);
        // first tick fires immediately — that's the contract we want
        // (`main.rs` calls spawn_gc once at startup, so the first sweep
        // runs in the first 6 hours, not after a 6-hour delay). Keep the
        // default behaviour.
        loop {
            ticker.tick().await;
            run_gc_once(&snapshot_dir());
        }
    })
}

/// one GC pass. Public so tests can drive it deterministically without
/// waiting on a tokio timer.
pub fn run_gc_once(dir: &Path) {
    let read_dir = match std::fs::read_dir(dir) {
        Ok(rd) => rd,
        Err(e) => {
            tracing::warn!(error = ?e, dir = %dir.display(), "snapshot GC: read_dir failed");
            return;
        }
    };
    let now = Utc::now();
    for entry in read_dir {
        let entry = match entry {
            Ok(e) => e,
            Err(e) => {
                tracing::warn!(error = ?e, "snapshot GC: dir entry read failed");
                continue;
            }
        };
        let path = entry.path();
        // skip non-files, non-`.json`, and the `.<id>.json.tmp` siblings
        // create_snapshot might leave behind on a crash mid-rename.
        let name = match path.file_name().and_then(|n| n.to_str()) {
            Some(n) => n,
            None => continue,
        };
        if name.starts_with('.') || !name.ends_with(".json") {
            continue;
        }
        let bytes = match std::fs::read(&path) {
            Ok(b) => b,
            Err(e) => {
                tracing::warn!(error = ?e, path = %path.display(), "snapshot GC: read failed");
                continue;
            }
        };
        let record: SnapshotRecord = match serde_json::from_slice(&bytes) {
            Ok(r) => r,
            Err(e) => {
                tracing::warn!(error = ?e, path = %path.display(), "snapshot GC: parse failed");
                continue;
            }
        };
        if record.expires_at <= now {
            if let Err(e) = std::fs::remove_file(&path) {
                tracing::warn!(error = ?e, path = %path.display(), "snapshot GC: unlink failed");
            } else {
                tracing::debug!(snapshot_id = %record.snapshot_id, "snapshot GC: removed expired");
            }
        }
    }
}

// tests

#[cfg(test)]
mod tests {
    use super::*;

    /// fixed-input id generation: emits 8 Crockford base32 chars and only
    /// uses the alphabet (no I/L/O/U).
    #[test]
    fn id_from_u64_uses_crockford_alphabet_only() {
        let id = id_from_u64(0);
        assert_eq!(id, "00000000");
        let id = id_from_u64(0xFF_FFFF_FFFF);
        assert_eq!(id, "ZZZZZZZZ");
        // every char is in the alphabet.
        for c in id.chars() {
            assert!(
                CROCKFORD_ALPHABET.contains(&(c as u8)),
                "char {c:?} not in Crockford alphabet"
            );
        }
    }

    /// high bits above 40 are masked off — same id whether or not those
    /// bits are set. Locks the spec's "low 40 bits" requirement.
    #[test]
    fn id_from_u64_masks_high_bits() {
        let a = id_from_u64(0xDEAD_BEEF_CAFE_1234);
        let b = id_from_u64(a_low40(0xDEAD_BEEF_CAFE_1234));
        assert_eq!(a, b);
    }

    fn a_low40(v: u64) -> u64 {
        v & 0xFF_FFFF_FFFF
    }

    /// regex accepts only 8-char Crockford ids (no I/L/O/U).
    #[test]
    fn id_regex_accepts_only_crockford_8char() {
        let re = id_regex();
        assert!(re.is_match("00000000"));
        assert!(re.is_match("ABCDEFGH"));
        assert!(re.is_match("ZYXWVTSR"));
        // wrong length
        assert!(!re.is_match("0000000"));
        assert!(!re.is_match("000000000"));
        // forbidden chars
        assert!(!re.is_match("ILOU0000"));
        assert!(!re.is_match("I0000000"));
        assert!(!re.is_match("L0000000"));
        assert!(!re.is_match("O0000000"));
        assert!(!re.is_match("U0000000"));
        // lowercase
        assert!(!re.is_match("abcdefgh"));
        // path traversal
        assert!(!re.is_match("../000000"));
    }
}
