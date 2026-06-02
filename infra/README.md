<!--
SPDX-License-Identifier: Apache-2.0
Copyright 2026 OZ Policy Builder contributors

Phase 10 Stream C — hosted-MCP deploy blueprint. See plan.md §Phase 10
"Hosted MCP endpoint" for the rationale: agent operators who don't want
to run STDIO locally need an HTTP endpoint, and shipping a thin IaC
blueprint (rather than a fully-managed service) keeps the trust surface
inside the operator's own cloud account.
-->

# `infra/` — Hosted MCP deployment blueprint

This directory holds **opt-in** infrastructure-as-code for running
`oz-policy-mcp` over Streamable HTTP behind a public TLS endpoint. The
OZ Accounts Policy Builder ships first and foremost as a local STDIO
MCP server — every walkthrough in `docs/walkthroughs/` works that way
with no cloud account, no DNS, and no TLS cert.

If you want a hosted endpoint (e.g., to wire a chat agent into the MCP
server without each user installing a Rust binary), this directory shows
one tested way to do it.

## Why this is HUMAN-REQUIRED

**No automation in this repository will ever run a paid cloud deploy on
your behalf.** Standing up a hosted endpoint is a deliberate operator
decision that involves at least the following human steps:

1. **Cloud account.** Sign up with the hosting provider, accept the
   provider's terms of service, register a billing method, and prove
   email/identity ownership. None of this is scriptable.
2. **DNS.** Allocate a hostname (subdomain or root domain) and configure
   the appropriate `A` / `AAAA` / `CNAME` records.
3. **TLS certificate.** Provision a certificate for that hostname. The
   recommended Fly.io blueprint below uses Fly's built-in Let's Encrypt
   integration, which still requires you to run `fly certs add` once
   per hostname.
4. **Bearer token.** Mint a long random value (e.g.,
   `openssl rand -hex 32`) and store it as a provider secret. The MCP
   server reads it from the `OZ_POLICY_MCP_TOKEN` environment variable;
   every request to `POST /mcp` must present it in the `Authorization:
   Bearer …` header.
5. **Recurring billing.** Even a free-tier deployment can accrue
   charges if traffic exceeds the included quotas. Monitor the
   provider's billing dashboard, not just the deploy logs.

The OZ Policy Builder maintainers do **not** host an endpoint on your
behalf. Every byte of cost, latency, and trust belongs to the operator
running the deploy.

## Why Fly.io is the recommended starting point

We benchmarked four options for hosting a small, mostly-idle Rust HTTP
service: Fly.io, Render, Railway, and AWS Fargate. Fly.io wins on the
metrics that matter for this workload:

- **Low friction.** `flyctl` is a single binary; `fly launch` reads a
  `fly.toml` file and bootstraps DNS + TLS + a default Postgres-free
  app skeleton in one command.
- **Cheap idle.** The default `auto_stop_machines = true` setting puts
  the machine to sleep after a few minutes of no traffic; cold-start is
  ~1 second for a Rust binary this small. Steady-state monthly cost on
  a single `shared-cpu-1x` VM is well under USD 5.
- **Built-in Anycast TLS.** Fly auto-provisions a Let's Encrypt cert
  per hostname; no Certbot, no nginx config, no DNS-01 round trip.
- **Region pinning.** `primary_region = "iad"` (or pick another) keeps
  the latency story honest; you control which jurisdiction your users'
  policy traffic transits.

If you prefer a different provider (Render, Railway, Fargate,
Kubernetes), the `Dockerfile.runtime` in `infra/fly/` is portable —
only `fly.toml` and `deploy.sh` are Fly-specific.

## What's in `infra/fly/`

- **`fly.toml`** — the Fly app manifest. App name (`oz-policy-builder`)
  and region (`iad`) are placeholders; rename to taste before you run
  `fly launch`.
- **`Dockerfile.runtime`** — multi-stage build: a Rust 1.89.0 stage
  compiles `oz-policy-mcp` in release mode, then a slim Debian
  bookworm runtime stage carries only the binary plus
  `ca-certificates`. No shell, no package manager, no build tooling in
  the final image.
- **`deploy.sh`** — thin wrapper around `fly deploy --remote-only`. Use
  it as-is or as a template for your own CI step.

## How to deploy (one-time setup)

These steps run on your workstation, **not in CI**. Nothing in this
repository will execute them for you.

```bash
# 1. Install flyctl (https://fly.io/docs/flyctl/installing/).
curl -L https://fly.io/install.sh | sh

# 2. Authenticate. This opens a browser; you must sign in or sign up.
fly auth login

# 3. (First deploy only) create the app from the manifest. Replace
#    `oz-policy-builder` with the unique name you want to register.
cd infra/fly
fly launch --name <your-unique-app-name> --copy-config --no-deploy

# 4. Mint and set the bearer token Fly will inject as OZ_POLICY_MCP_TOKEN.
#    Use `openssl rand -hex 32` or any high-entropy source.
fly secrets set OZ_POLICY_MCP_TOKEN="$(openssl rand -hex 32)"

# 5. Deploy.
./deploy.sh
```

After the first deploy, `./deploy.sh` is the only command you re-run
for subsequent updates. Add `fly certs add mcp.<your-domain>` once if
you want a custom hostname instead of the `*.fly.dev` default.

## Health check

The MCP server exposes `GET /healthz` with no auth requirement. Fly's
HTTP health checks (configured in `fly.toml`) poll it every 30 s. If
you're putting another load balancer in front, point it at the same
path; do **not** route `GET /healthz` through your bearer-auth layer
or you'll lock yourself out of the readiness signal.

## Threat-model reminder

A hosted MCP endpoint inherits the same security posture as the
local STDIO server, except every step in `SECURITY.md` §"Operating an
HTTP transport" now applies to *you*, the operator. In particular:

- The bearer token is the **only** authentication boundary; rotate it
  if you ever paste it into a tool you don't fully trust.
- The server never holds private keys. Wallet signing happens in the
  client, exactly as in the STDIO path.
- Mainnet remains opt-in. The MCP server does not embed any RPC URL by
  default; clients pass `network` and `rpc_url` on each tool call.
