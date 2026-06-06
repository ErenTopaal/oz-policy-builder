#!/usr/bin/env bash
# re-derive every committed walkthrough wasm and check pinned hashes.
# usage: scripts/reproducible-build.sh [RELEASE_TAG]
# hard fail on: missing tool, version drift, build failure, hash mismatch.
# success writes `reproducible-build-manifest-<tag>.json` at workspace root.

set -euo pipefail

WORKTREE="$(git rev-parse --show-toplevel)"
cd "$WORKTREE"

RELEASE_TAG="${1:-untagged-$(date -u +%Y-%m-%dT%H%M%SZ)}"
MANIFEST="$WORKTREE/reproducible-build-manifest-$RELEASE_TAG.json"

# expected pins — drift here is a release blocker.
EXPECTED_RUSTC="1.89.0"
EXPECTED_STELLAR_CLI="25.1.0"
# wasm-opt is embedded inside stellar-cli; not surfaced by `stellar --help`.
EXPECTED_WASM_OPT="0.116.1"
EXPECTED_BINARYEN="116"

log()  { printf '[reproducible-build] %s\n' "$*" >&2; }
fail() { printf '[reproducible-build][FATAL] %s\n' "$*" >&2; exit 1; }

# portable sha-256: linux ships sha256sum, macos ships shasum.
sha256_file() {
    if command -v sha256sum >/dev/null 2>&1; then
        sha256sum "$1" | awk '{print $1}'
    elif command -v shasum >/dev/null 2>&1; then
        shasum -a 256 "$1" | awk '{print $1}'
    else
        fail "neither sha256sum nor shasum is on PATH"
    fi
}

# 1. capture environment fingerprint.

log "validating toolchain"

for bin in rustc cargo stellar jq git; do
    command -v "$bin" >/dev/null 2>&1 || fail "$bin is not on PATH"
done

# `rustc --version` → `rustc 1.89.0 (29483883e 2025-08-04)`.
RUST_VERSION_FULL="$(rustc --version)"
RUST_VERSION_NUM="$(printf '%s' "$RUST_VERSION_FULL" | awk '{print $2}')"
if [ "$RUST_VERSION_NUM" != "$EXPECTED_RUSTC" ]; then
    fail "rustc version drift: expected $EXPECTED_RUSTC, got $RUST_VERSION_NUM (full: $RUST_VERSION_FULL)"
fi

# `stellar --version` first line is `stellar <ver>`.
STELLAR_CLI_VERSION_FULL="$(stellar --version | head -n1)"
STELLAR_CLI_VERSION_NUM="$(printf '%s' "$STELLAR_CLI_VERSION_FULL" | awk '{print $2}')"
if [ "$STELLAR_CLI_VERSION_NUM" != "$EXPECTED_STELLAR_CLI" ]; then
    fail "stellar-cli version drift: expected $EXPECTED_STELLAR_CLI, got $STELLAR_CLI_VERSION_NUM (full: $STELLAR_CLI_VERSION_FULL)"
fi

TOOLCHAIN_SHA="$(sha256_file rust-toolchain.toml)"
CARGO_TOML_SHA="$(sha256_file Cargo.toml)"
CARGO_LOCK_SHA="$(sha256_file Cargo.lock)"
DOCKERFILE_SHA="$(sha256_file ci/Dockerfile)"
DENY_TOML_SHA="$(sha256_file deny.toml)"
SANDBOX_PROFILE_SHA="$(sha256_file scripts/sandbox-profile-macos.sb)"

log "rustc                = $RUST_VERSION_FULL"
log "stellar-cli          = $STELLAR_CLI_VERSION_FULL"
log "wasm-opt (embedded)  = $EXPECTED_WASM_OPT (Binaryen $EXPECTED_BINARYEN)"
log "rust-toolchain.toml  = sha256:$TOOLCHAIN_SHA"
log "Cargo.toml           = sha256:$CARGO_TOML_SHA"
log "Cargo.lock           = sha256:$CARGO_LOCK_SHA"
log "ci/Dockerfile        = sha256:$DOCKERFILE_SHA"
log "deny.toml            = sha256:$DENY_TOML_SHA"
log "sandbox-profile      = sha256:$SANDBOX_PROFILE_SHA"

# 2. discover pinned wasm hashes by scanning `wasm_hash.txt` next to specs.
# parallel arrays so the script runs on macos bash 3.2 too.
SPECS=()
SLOT_DIRS=()
SLOT_NAMES=()

# numbered walkthroughs; spec marker is `expected-spec-auto.json`.
shopt -s nullglob
for dir in walkthroughs/[0-9][0-9]-*/; do
    spec="${dir}expected-spec-auto.json"
    [ -f "$spec" ] || continue
    for hashfile in "${dir}wasm/"slot_*/wasm_hash.txt; do
        slot_dir="$(dirname "$hashfile")"
        slot_name="$(basename "$slot_dir")"
        SPECS+=("$spec")
        SLOT_DIRS+=("$slot_dir")
        SLOT_NAMES+=("$slot_name")
    done
done

# codegen fixture: spec is `spec.json`, slots under `expected/`.
phase3_dir="walkthroughs/phase3-codegen-fixture"
if [ -f "$phase3_dir/spec.json" ]; then
    for hashfile in "$phase3_dir/expected/"slot_*/wasm_hash.txt; do
        [ -f "$hashfile" ] || continue
        slot_dir="$(dirname "$hashfile")"
        slot_name="$(basename "$slot_dir")"
        SPECS+=("$phase3_dir/spec.json")
        SLOT_DIRS+=("$slot_dir")
        SLOT_NAMES+=("$slot_name")
    done
fi
shopt -u nullglob

if [ "${#SPECS[@]}" -eq 0 ]; then
    fail "no pinned wasm_hash.txt files discovered — refusing to emit empty manifest"
fi

log "discovered ${#SPECS[@]} pinned WASM(s) to verify"

# 3. build the cli release. set OZ_REPRODUCIBLE_BUILD_CLEAN=1 to force cargo clean.
if [ "${OZ_REPRODUCIBLE_BUILD_CLEAN:-0}" = "1" ]; then
    log "OZ_REPRODUCIBLE_BUILD_CLEAN=1 — running cargo clean"
    cargo clean
fi

log "building oz-policy-cli (release)"
cargo build --release -p oz-policy-cli --locked
CLI_BIN="$WORKTREE/target/release/oz-policy-cli"
[ -x "$CLI_BIN" ] || fail "expected $CLI_BIN to be executable after cargo build"

# 4. re-derive every wasm and verify hash. codegen runs once per spec.

declare -a UNIQUE_SPECS=()
for s in "${SPECS[@]}"; do
    seen=0
    for u in "${UNIQUE_SPECS[@]:-}"; do
        [ "$s" = "$u" ] && { seen=1; break; }
    done
    [ "$seen" -eq 0 ] && UNIQUE_SPECS+=("$s")
done

# per-slot entries → temp file; jq's final pass assembles the manifest.
SLOTS_JSON_TMP="$(mktemp)"
trap 'rm -f "$SLOTS_JSON_TMP"' EXIT
printf '[' > "$SLOTS_JSON_TMP"
slots_first_entry=1

mismatch_count=0

for spec in "${UNIQUE_SPECS[@]}"; do
    out_dir="$(mktemp -d)"
    log "codegen $spec → $out_dir"
    if ! "$CLI_BIN" codegen "$spec" --out "$out_dir" > /dev/null; then
        rm -rf "$out_dir"
        fail "oz-policy-cli codegen $spec exited non-zero"
    fi

    # every committed slot that belongs to this spec.
    for i in "${!SPECS[@]}"; do
        [ "${SPECS[$i]}" = "$spec" ] || continue
        slot_dir="${SLOT_DIRS[$i]}"
        slot_name="${SLOT_NAMES[$i]}"
        pinned_hash="$(tr -d '[:space:]' < "$slot_dir/wasm_hash.txt")"

        rederived_wasm="$out_dir/$slot_name/policy.wasm"
        if [ ! -f "$rederived_wasm" ]; then
            printf '[reproducible-build][FATAL] codegen produced no %s/policy.wasm for spec %s\n' \
                "$slot_name" "$spec" >&2
            mismatch_count=$((mismatch_count + 1))
            continue
        fi
        actual_hash="$(sha256_file "$rederived_wasm")"

        if [ "$actual_hash" = "$pinned_hash" ]; then
            log "[ok]    $slot_dir → $actual_hash"
        else
            printf '[reproducible-build][HASH MISMATCH] %s\n' "$slot_dir" >&2
            printf '    expected (pinned in wasm_hash.txt): %s\n' "$pinned_hash" >&2
            printf '    actual   (re-derived):              %s\n' "$actual_hash"  >&2
            mismatch_count=$((mismatch_count + 1))
        fi

        # emit a json object per slot, comma-separated.
        if [ "$slots_first_entry" -eq 0 ]; then
            printf ',' >> "$SLOTS_JSON_TMP"
        fi
        slots_first_entry=0
        jq -nc \
            --arg spec_path "$spec" \
            --arg slot_dir "$slot_dir" \
            --arg slot_name "$slot_name" \
            --arg pinned "$pinned_hash" \
            --arg actual "$actual_hash" \
            '{spec_path: $spec_path, slot_dir: $slot_dir, slot_name: $slot_name,
              pinned_sha256: $pinned, actual_sha256: $actual,
              matches: ($pinned == $actual)}' \
            >> "$SLOTS_JSON_TMP"
    done

    rm -rf "$out_dir"
done

printf ']' >> "$SLOTS_JSON_TMP"

if [ "$mismatch_count" -ne 0 ]; then
    fail "$mismatch_count WASM hash mismatch(es); refusing to emit manifest"
fi

# 5. emit the manifest.

GIT_COMMIT="$(git rev-parse HEAD)"
GIT_DIRTY="$( [ -n "$(git status --porcelain)" ] && printf 'true' || printf 'false' )"
GENERATED_AT="$(date -u +%Y-%m-%dT%H:%M:%SZ)"

jq -n \
  --arg release_tag         "$RELEASE_TAG" \
  --arg generated_at        "$GENERATED_AT" \
  --arg git_commit          "$GIT_COMMIT" \
  --argjson git_dirty       "$GIT_DIRTY" \
  --arg rust_full           "$RUST_VERSION_FULL" \
  --arg rust_version        "$RUST_VERSION_NUM" \
  --arg stellar_cli_full    "$STELLAR_CLI_VERSION_FULL" \
  --arg stellar_cli_version "$STELLAR_CLI_VERSION_NUM" \
  --arg wasm_opt_version    "$EXPECTED_WASM_OPT" \
  --arg binaryen_version    "$EXPECTED_BINARYEN" \
  --arg toolchain_sha       "$TOOLCHAIN_SHA" \
  --arg cargo_toml_sha      "$CARGO_TOML_SHA" \
  --arg cargo_lock_sha      "$CARGO_LOCK_SHA" \
  --arg dockerfile_sha      "$DOCKERFILE_SHA" \
  --arg deny_toml_sha       "$DENY_TOML_SHA" \
  --arg sandbox_profile_sha "$SANDBOX_PROFILE_SHA" \
  --slurpfile pinned_wasms  "$SLOTS_JSON_TMP" \
  '{
     release_tag: $release_tag,
     generated_at: $generated_at,
     git: { commit: $git_commit, dirty: $git_dirty },
     toolchain: {
       rust:        { version: $rust_version, banner: $rust_full },
       stellar_cli: { version: $stellar_cli_version, banner: $stellar_cli_full },
       wasm_opt:    { version: $wasm_opt_version, binaryen: $binaryen_version,
                      source: "embedded in stellar-cli" }
     },
     input_hashes: {
       "rust-toolchain.toml":            $toolchain_sha,
       "Cargo.toml":                     $cargo_toml_sha,
       "Cargo.lock":                     $cargo_lock_sha,
       "ci/Dockerfile":                  $dockerfile_sha,
       "deny.toml":                      $deny_toml_sha,
       "scripts/sandbox-profile-macos.sb": $sandbox_profile_sha
     },
     pinned_wasms: $pinned_wasms[0]
   }' > "$MANIFEST"

log "manifest: $MANIFEST"
log "reproducible build OK ($(jq '.pinned_wasms | length' "$MANIFEST") pinned WASM(s) verified)"
