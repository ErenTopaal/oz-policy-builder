# syntax=docker/dockerfile:1.7
#
# Reproducible-build image for the OZ Accounts Policy Builder.
#
# Pins every input to the same versions the workspace uses locally and in CI:
#
#   * Base image      — rust:1.89.0-slim-bookworm
#                       (manifest digest sha256:d7fc7de78bb8c1469933aeecbf801314d30d7d6e9f0578bba4cfa285bfa37fe6
#                        per `docker manifest inspect` 2026-05-15;
#                        per-arch: amd64=sha256:294917190b5a3fed18d3303213943f40ec9b644b3cd6e5cdc8cd029334a770b3,
#                                  arm64=sha256:86e85c823cebcb7b63777a67441d4cf4e7b082738bf36962e21d13596177ee3b)
#   * rustc           — 1.89.0   (locked by rust-toolchain.toml + the base tag)
#   * cargo-nextest   — 0.9.128  (last release compatible with rustc 1.89.0)
#   * cargo-deny      — 0.19.6   (matches `.github/workflows/ci.yml`)
#   * cargo-fuzz      — 0.13.1   (Stream A's harness — see plan.md Phase 9)
#   * stellar-cli     — 25.1.0   (linux tarballs from
#                       https://github.com/stellar/stellar-cli/releases/tag/v25.1.0;
#                       SHA-256 of the two linux tarballs is verified below)
#
# The stellar-cli binary embeds wasm-opt = 0.116.1 (Binaryen v116) via the
# `additional-libs` feature — see docs/oz-internal-shapes.md
# "Reproducible-build prereqs". No system wasm-opt is invoked.
#
# Pin the base image by digest so a re-tag upstream cannot silently change
# the build inputs. The `@sha256:…` form is the manifest-list digest, which
# Docker resolves to the correct per-arch image automatically.
FROM rust:1.89.0-slim-bookworm@sha256:d7fc7de78bb8c1469933aeecbf801314d30d7d6e9f0578bba4cfa285bfa37fe6

# Avoid interactive apt prompts; fail loudly on pipe errors.
ENV DEBIAN_FRONTEND=noninteractive
SHELL ["/bin/bash", "-o", "pipefail", "-c"]

# System packages:
#   * build-essential  — cc/ld for `cargo build` (the slim base ships rustc but
#                        not gcc; soroban-env-host's transitive crates need a
#                        linker).
#   * ca-certificates  — TLS roots so the curl below + cargo registry fetches
#                        verify upstream certs.
#   * curl             — fetch the stellar-cli tarball.
#   * jq               — required by scripts/reproducible-build.sh to emit the
#                        manifest JSON.
#   * bubblewrap       — `bwrap`; codegen's Linux sandbox driver (Phase 3,
#                        `crates/oz-policy-codegen/src/sandbox.rs`).
#   * coreutils        — sha256sum for the script's hash assertions (already
#                        present in bookworm but pinned-installed for clarity).
RUN apt-get update \
 && apt-get install -y --no-install-recommends \
        build-essential \
        ca-certificates \
        curl \
        jq \
        bubblewrap \
        coreutils \
 && rm -rf /var/lib/apt/lists/*

# Install the WASM target the workspace's release build needs. The Rust
# toolchain in the base image is the stable channel pinned at 1.89.0; the
# wasm32-unknown-unknown target is not preinstalled.
RUN rustup target add wasm32-unknown-unknown

# Pinned auxiliary cargo binaries. `--locked` ensures we honour each tool's
# committed Cargo.lock — the same flag `.github/workflows/ci.yml` uses — and
# the leading `=` operator in `--version` forces exact-version selection so a
# Cargo registry yank or new patch release cannot silently drift the image.
RUN cargo install cargo-nextest --version =0.9.128 --locked \
 && cargo install cargo-deny    --version =0.19.6  --locked \
 && cargo install cargo-fuzz    --version =0.13.1  --locked

# stellar-cli v25.1.0. Two architectures shipped by upstream as linux
# tarballs; we install whichever matches `TARGETARCH` (docker BuildKit's
# auto-populated platform var). SHA-256 is verified against the upstream
# tarball — values captured 2026-05-15 by sha256-ing the released artifacts:
#
#   stellar-cli-25.1.0-aarch64-unknown-linux-gnu.tar.gz
#     15c68afadbc6bac966809bd3bfff2a6f3c10da38df9453cfde25645cab474dfc
#   stellar-cli-25.1.0-x86_64-unknown-linux-gnu.tar.gz
#     e6fac619b2ae9b3ecb843a9e8e3bfc94dce79e0b73c63cb1cbbd08682bb0a0ba
ARG TARGETARCH
ARG STELLAR_CLI_VERSION=25.1.0
ARG STELLAR_CLI_SHA256_AMD64=e6fac619b2ae9b3ecb843a9e8e3bfc94dce79e0b73c63cb1cbbd08682bb0a0ba
ARG STELLAR_CLI_SHA256_ARM64=15c68afadbc6bac966809bd3bfff2a6f3c10da38df9453cfde25645cab474dfc
RUN set -euo pipefail \
 && case "${TARGETARCH:-amd64}" in \
        amd64) triple=x86_64-unknown-linux-gnu;  expected_sha=${STELLAR_CLI_SHA256_AMD64} ;; \
        arm64) triple=aarch64-unknown-linux-gnu; expected_sha=${STELLAR_CLI_SHA256_ARM64} ;; \
        *) echo "unsupported TARGETARCH=${TARGETARCH}" >&2; exit 1 ;; \
    esac \
 && tarball="stellar-cli-${STELLAR_CLI_VERSION}-${triple}.tar.gz" \
 && curl -fsSL "https://github.com/stellar/stellar-cli/releases/download/v${STELLAR_CLI_VERSION}/${tarball}" -o "/tmp/${tarball}" \
 && echo "${expected_sha}  /tmp/${tarball}" | sha256sum -c - \
 && tar -xzf "/tmp/${tarball}" -C /usr/local/bin stellar \
 && rm -f "/tmp/${tarball}" \
 && /usr/local/bin/stellar --version

# Record the pinned tool versions inside the image for downstream debugging.
# `reproducible-build.sh` reads these via `stellar --version`, `rustc
# --version`, etc., but having them baked in as image labels makes
# `docker inspect` self-documenting.
LABEL org.opencontainers.image.title="oz-policy-builder-reproducible-build" \
      org.opencontainers.image.description="Reproducible build image for OZ Accounts Policy Builder (Phase 9)" \
      org.opencontainers.image.licenses="Apache-2.0" \
      oz.policy.builder.rust="1.89.0" \
      oz.policy.builder.stellar-cli="25.1.0" \
      oz.policy.builder.wasm-opt="0.116.1" \
      oz.policy.builder.binaryen="116" \
      oz.policy.builder.cargo-nextest="0.9.128" \
      oz.policy.builder.cargo-deny="0.19.6" \
      oz.policy.builder.cargo-fuzz="0.13.1"

WORKDIR /work

# Default entrypoint is a plain bash so callers can invoke any of:
#   docker run --rm -v "$PWD:/work" <img> -c "scripts/reproducible-build.sh"
#   docker run --rm -it -v "$PWD:/work" <img>
ENTRYPOINT ["/bin/bash"]
