# syntax=docker/dockerfile:1.7
# reproducible-build image. pinned: rustc 1.89.0, cargo-nextest 0.9.128,
# cargo-deny 0.19.6, cargo-fuzz 0.13.1, stellar-cli 25.1.0 (embeds wasm-opt
# 0.116.1 / binaryen 116). base image pinned by manifest-list digest.
FROM rust:1.89.0-slim-bookworm@sha256:d7fc7de78bb8c1469933aeecbf801314d30d7d6e9f0578bba4cfa285bfa37fe6

# noninteractive apt, fail-loud pipes.
ENV DEBIAN_FRONTEND=noninteractive
SHELL ["/bin/bash", "-o", "pipefail", "-c"]

# system deps: build-essential (linker), ca-certs (tls), curl (stellar tarball),
# jq (manifest), bubblewrap (linux sandbox), coreutils (sha256sum).
RUN apt-get update \
 && apt-get install -y --no-install-recommends \
        build-essential \
        ca-certificates \
        curl \
        jq \
        bubblewrap \
        coreutils \
 && rm -rf /var/lib/apt/lists/*

# wasm32 target not preinstalled in the slim base.
RUN rustup target add wasm32-unknown-unknown

# pinned aux cargo binaries; `=` + `--locked` block drift.
RUN cargo install cargo-nextest --version =0.9.128 --locked \
 && cargo install cargo-deny    --version =0.19.6  --locked \
 && cargo install cargo-fuzz    --version =0.13.1  --locked

# stellar-cli 25.1.0; sha-256 verified per arch.
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

# oci labels — self-documenting `docker inspect`.
LABEL org.opencontainers.image.title="oz-policy-builder-reproducible-build" \
      org.opencontainers.image.description="reproducible-build image" \
      org.opencontainers.image.licenses="Apache-2.0" \
      oz.policy.builder.rust="1.89.0" \
      oz.policy.builder.stellar-cli="25.1.0" \
      oz.policy.builder.wasm-opt="0.116.1" \
      oz.policy.builder.binaryen="116" \
      oz.policy.builder.cargo-nextest="0.9.128" \
      oz.policy.builder.cargo-deny="0.19.6" \
      oz.policy.builder.cargo-fuzz="0.13.1"

WORKDIR /work

# bash entrypoint so `docker run … -c "scripts/reproducible-build.sh"` works.
ENTRYPOINT ["/bin/bash"]
