#!/usr/bin/env bash
# thin wrapper around `fly deploy`. operator handles `fly auth`/`secrets`/`certs`.
# usage: `./deploy.sh` or `APP=my-app ./deploy.sh`.

set -euo pipefail

# resolve script dir so it works regardless of cwd.
script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$script_dir"

APP="${APP:-oz-policy-builder}"

# warn if OZ_POLICY_MCP_TOKEN secret isn't set.
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

# --remote-only builds on Fly's vm; no local docker needed.
exec fly deploy \
    --app "$APP" \
    --config fly.toml \
    --remote-only \
    "$@"
