#!/usr/bin/env bash
# deploy.sh — thin wrapper around `fly deploy` for the hosted MCP
# endpoint. See `infra/README.md` for the prerequisite human steps
# (auth, secrets, DNS, certs).
#
# Usage:
#   ./deploy.sh                  # uses the app name in fly.toml
#   APP=my-app ./deploy.sh       # override the app name
#
# This script intentionally does NOT do `fly auth`, `fly secrets set`,
# or `fly certs add` for you. Those are operator decisions; running them
# from automation would mean this repository's CI could provision real
# infrastructure on the operator's behalf.

set -euo pipefail

# Resolve script directory so the command works regardless of cwd.
script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$script_dir"

# Default app name lives in fly.toml (`oz-policy-builder`). Allow an
# environment override for operators who renamed the app.
APP="${APP:-oz-policy-builder}"

# Sanity-check: warn loudly if the OZ_POLICY_MCP_TOKEN secret is not set.
# Fly's `fly secrets list` outputs one secret per line; we grep for the
# exact name.
if ! fly secrets list --app "$APP" 2>/dev/null | grep -q '^OZ_POLICY_MCP_TOKEN'; then
    cat <<'EOF' >&2
WARNING: OZ_POLICY_MCP_TOKEN is not set on this Fly app.

Set it with a high-entropy random value BEFORE deploying:

    fly secrets set OZ_POLICY_MCP_TOKEN="$(openssl rand -hex 32)"

Continuing the deploy anyway — the MCP binary will fail to start
without the secret, but that's a clearer failure mode than silently
running with a missing auth boundary.

EOF
fi

# `--remote-only` runs the build on Fly's builder VM, so the operator's
# workstation never needs Docker locally. `--config fly.toml` is the
# default but stated explicitly for clarity.
exec fly deploy \
    --app "$APP" \
    --config fly.toml \
    --remote-only \
    "$@"
