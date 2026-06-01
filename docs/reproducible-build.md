# Reproducible-build procedure

This document describes how to re-derive every committed walkthrough WASM
from source and verify byte-equality with the `wasm_hash.txt` files
pinned alongside each spec. The procedure is the binary completion gate
for [`plan.md` § "Phase 9 — Security / Audit / Hardening"](../plan.md):
**any third party must be able to re-derive the exact published WASM
hashes from source, starting from a fresh git clone, in under an hour,
using only this document and the pinned tools listed below.**

Three artifacts are involved:

* `scripts/reproducible-build.sh` — the verifier.
* `ci/Dockerfile` — the hermetic build environment.
* `.github/workflows/reproducible-build.yml` — runs the verifier on every
  release tag and attaches the manifest as a release asset.

---

## Pinned tool versions

Every tool that participates in the codegen pipeline is version-pinned.
If a host machine has even a patch-version drift, the script refuses to
emit a manifest (it exits with a `[FATAL]` line on stderr).

| Tool          | Version       | Source                                                                       | Notes                                                                                                                                                          |
| ------------- | ------------- | ---------------------------------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Rust toolchain | `1.89.0`      | `rust-toolchain.toml` (`channel = "1.89.0"`)                                  | Satisfies MSRV of every workspace dep. Channel = stable.                                                                                                       |
| `wasm-opt`    | `0.116.1`     | Embedded in `stellar-cli` v25.1.0                                            | Binaryen `116`. Verified at `https://github.com/stellar/stellar-cli/blob/v25.1.0/cmd/soroban-cli/Cargo.toml` (`wasm-opt = "=0.116.1"`).                          |
| Binaryen      | `116`         | Embedded in `wasm-opt = 0.116.1`                                             | Optimizer version is locked to the CLI version, NOT to the host's system `wasm-opt`. See [`oz-internal-shapes.md` § "Reproducible-build prereqs"](./oz-internal-shapes.md#reproducible-build-prereqs). |
| `stellar-cli` | `25.1.0`      | `github.com/stellar/stellar-cli/releases/tag/v25.1.0`                        | Tarball SHA-256s are verified by `ci/Dockerfile` at image-build time (see the `RUN curl … sha256sum -c -` block).                                              |
| `cargo-nextest` | `0.9.128`   | crates.io                                                                    | Last release compatible with rustc 1.89.0 (0.9.129 bumped MSRV to 1.91).                                                                                       |
| `cargo-deny`  | `0.19.6`      | crates.io                                                                    | Matches `.github/workflows/ci.yml`.                                                                                                                            |
| `cargo-fuzz`  | `0.13.1`      | crates.io                                                                    | Stream A's libFuzzer harness; not invoked by the reproducible-build flow, but installed in the image so the fuzz CI step can re-use it.                          |
| `rust:1.89.0-slim-bookworm` base image | manifest digest `sha256:d7fc7de78bb8c1469933aeecbf801314d30d7d6e9f0578bba4cfa285bfa37fe6` | Docker Hub | Pinned in `ci/Dockerfile` via the `FROM rust:1.89.0-slim-bookworm@sha256:…` form so a re-tag upstream cannot silently change the build inputs. Per-arch digests are recorded in the Dockerfile comment block. |

### Verified tarball SHA-256 (stellar-cli 25.1.0)

The `ci/Dockerfile` downloads the upstream Linux tarballs and verifies them
with `sha256sum -c -` before extracting the `stellar` binary. The expected
SHA-256 values are pinned in the Dockerfile and reproduced here for
auditors:

| Asset                                                  | SHA-256                                                              |
| ------------------------------------------------------ | -------------------------------------------------------------------- |
| `stellar-cli-25.1.0-aarch64-unknown-linux-gnu.tar.gz`  | `15c68afadbc6bac966809bd3bfff2a6f3c10da38df9453cfde25645cab474dfc`   |
| `stellar-cli-25.1.0-x86_64-unknown-linux-gnu.tar.gz`   | `e6fac619b2ae9b3ecb843a9e8e3bfc94dce79e0b73c63cb1cbbd08682bb0a0ba`   |

These values were captured on **2026-05-15** by running `sha256sum` on the
released artifacts at `https://github.com/stellar/stellar-cli/releases/download/v25.1.0/`.

---

## How to run locally

### Option 1 — Native (recommended for day-to-day local verification)

Requires the pinned tools on your host `PATH`. macOS users already have
`stellar-cli` installed via the workspace's local toolchain setup (`brew
install stellar/tap/stellar-cli` ⟶ pin to `25.1.0`); Linux users can
install via the upstream tarball with the SHA verification step from
`ci/Dockerfile`.

```bash
git clone https://github.com/oz-policy-builder/oz-policy-builder
cd oz-policy-builder
./scripts/reproducible-build.sh local-$(date +%Y-%m-%d)
```

On success the script prints `reproducible build OK (N pinned WASM(s)
verified)` and writes the manifest to
`reproducible-build-manifest-local-<date>.json` at the workspace root.

On a hash mismatch the script exits non-zero and prints both the pinned
hash (from `wasm_hash.txt`) and the re-derived hash, allowing the diff to
be diagnosed without re-running the build.

### Option 2 — Docker (matches the CI environment exactly)

Use this when reviewing a release or filing a "didn't reproduce" report,
since this is the configuration the published manifest was generated in.

```bash
git clone https://github.com/oz-policy-builder/oz-policy-builder
cd oz-policy-builder

docker build -t oz-policy-builder/reproducible:local -f ci/Dockerfile ci/

docker run --rm \
    -v "$PWD:/work" \
    oz-policy-builder/reproducible:local \
    -c "scripts/reproducible-build.sh local-$(date +%Y-%m-%d)"
```

Note: the `cargo` registry is fetched fresh inside the container on the
first run (a few minutes). For repeated runs, mount a host-side cache:

```bash
mkdir -p .docker-cargo-home
docker run --rm \
    -v "$PWD:/work" \
    -v "$PWD/.docker-cargo-home:/root/.cargo" \
    -e CARGO_HOME=/root/.cargo \
    oz-policy-builder/reproducible:local \
    -c "scripts/reproducible-build.sh local-$(date +%Y-%m-%d)"
```

---

## How to verify a published manifest

When a release tag is published, the `Reproducible Build` workflow runs
the script inside the `ci/Dockerfile` image and attaches the manifest
file (`reproducible-build-manifest-<release_tag>.json`) to the GitHub
Release. Third-party verifiers should:

1. Download both the release manifest and the source tarball
   (`Source code (tar.gz)`) from the GitHub Release page.
2. Re-run the verifier locally against the same release tag:
   ```bash
   tar xf v<tag>.tar.gz
   cd oz-policy-builder-<tag>
   ./scripts/reproducible-build.sh "v<tag>"
   ```
3. Diff the locally-emitted manifest against the published one:
   ```bash
   diff \
       <(jq -S . reproducible-build-manifest-v<tag>.json) \
       <(jq -S . path/to/downloaded/manifest.json)
   ```

The diff MUST be empty. Acceptable shapes of divergence (none of which
should ever appear in practice):

* `git.commit` differs → you cloned the wrong source ref.
* `git.dirty` differs → your worktree has uncommitted changes.
* `generated_at` differs → trivially expected (timestamp); this field is
  the only one in the manifest that is NOT load-bearing for byte
  equality.

Every other field MUST be byte-identical. If they aren't, please file an
issue with both manifests attached.

---

## Manifest schema

The manifest is a single JSON object with the following top-level keys:

```jsonc
{
  "release_tag":   "v0.1.0",
  "generated_at":  "2026-05-15T23:24:53Z",      // UTC RFC 3339; non-load-bearing
  "git": {
    "commit": "<40-char SHA>",
    "dirty":  false
  },
  "toolchain": {
    "rust":        { "version": "1.89.0",  "banner": "rustc 1.89.0 (29483883e 2025-08-04)" },
    "stellar_cli": { "version": "25.1.0",  "banner": "stellar 25.1.0" },
    "wasm_opt":    { "version": "0.116.1", "binaryen": "116",
                     "source": "embedded in stellar-cli; see docs/oz-internal-shapes.md" }
  },
  "input_hashes": {
    "rust-toolchain.toml":              "<sha256 hex>",
    "Cargo.toml":                       "<sha256 hex>",
    "Cargo.lock":                       "<sha256 hex>",
    "ci/Dockerfile":                    "<sha256 hex>",
    "deny.toml":                        "<sha256 hex>",
    "scripts/sandbox-profile-macos.sb": "<sha256 hex>"
  },
  "pinned_wasms": [
    {
      "spec_path":     "walkthroughs/01-blend-yield/expected-spec-auto.json",
      "slot_dir":      "walkthroughs/01-blend-yield/wasm/slot_0",
      "slot_name":     "slot_0",
      "pinned_sha256": "<hex from wasm_hash.txt>",
      "actual_sha256": "<hex of re-derived bytes>",
      "matches":       true
    }
    // … one entry per pinned WASM discovered under walkthroughs/
  ]
}
```

The manifest is intentionally self-contained: every value it references
either lives in the worktree (so reviewers can diff in place) or is a
documented constant (toolchain versions) backed by an upstream URL the
auditor can verify independently.

---

## How a hash mismatch is surfaced

`scripts/reproducible-build.sh` exits non-zero on the first mismatch and
prints both hashes to stderr. A failed CI run uploads a
`reproducible-build-failure-<release_tag>` artifact bundle containing:

* any partial manifest file the script wrote before failing;
* every committed `walkthroughs/**/wasm_hash.txt`;
* any `*.wasm` left behind under `CARGO_TARGET_DIR`.

The most common cause of a real mismatch is a sibling stream landing a
change that perturbs the codegen output (e.g. a template tweak in Stream
B's audit-lint integration, a new constraint in Stream A's fuzz corpus).
The fix is always the same: regenerate the affected walkthrough's
`wasm/` directory and commit the new `policy.wasm` + `wasm_hash.txt`
alongside the change that perturbed them. See each walkthrough's
`README.md` (`§ Reproducing the fixture`) for the canonical recipe.

---

## Caveats

* **Sandbox driver scope.** The macOS `sandbox-exec` profile and Linux
  `bwrap` invocation are hardening, not security barriers. Correctness of
  the build does not depend on them. They appear in the input-hash list
  only so a future audit can detect tampering with the sandbox config.
* **Caching is host-local.** The codegen pipeline writes a per-render
  cache under `$OZ_POLICY_CODEGEN_CACHE_DIR` (default
  `~/.cache/oz-policy-codegen/sandbox/<hex>`). The cache is hit-only —
  every cache miss recompiles from source — so a freshly cleared cache
  cannot produce a different WASM. The script always logs `sandbox cache
  hit` or `sandbox cache miss` per-slot for traceability.
* **No `cargo clean` by default.** Pass `OZ_REPRODUCIBLE_BUILD_CLEAN=1`
  to wipe the host `target/` directory before the verifier runs. CI does
  not pass this flag (the image has nothing cached anyway).
* **`generated_at` is the only non-deterministic field.** Every other
  field in the manifest is a pure function of the worktree at
  `git.commit`. If you need a fully-deterministic diff, run `jq 'del(.generated_at)'`
  on both files before diffing.
