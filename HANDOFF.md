# Handoff — What You Need to Do

The autonomous implementation phase is complete: 273 Rust tests + 80 wallet-adapter tests = **353 total passing**, all 4 phase-completion gates green, reproducible-build script re-derives all 3 walkthrough WASMs byte-equally. 139 commits on `phase-1-foundations`, tree clean.

Five items remain that genuinely require your input or action. They are ordered: each step unblocks the next.

> **Maintenance note:** This file is a living checklist for the bridge between autonomous implementation and production launch. Update it as you complete each step. Once Step 7 is done and verified, this file can be archived or deleted.

---

## Step 1 — Three trivial inputs (5 min, you tell me; I substitute)

Reply to me with:
1. **GitHub org or username** — e.g., `tolgayayci` or `oz-policy-builder`. Substitutes `<org-placeholder>` in `Cargo.toml`, `wallet-adapter/package.json`, `CHANGELOG.md`, `docs/reproducible-build.md`.
2. **Security contact email** — e.g., a fresh `security@your-domain` alias or your personal email. Substitutes `security@<placeholder.example>` in `SECURITY.md`, `CODE_OF_CONDUCT.md` (`conduct@<placeholder.example>` is a separate field — give two emails or one).
3. **GPG public key fingerprint** (optional in this step; required by Step 7) — the 40-hex-char fingerprint string from `gpg --list-keys --fingerprint`.

After substitution I commit and we move to Step 2.

---

## Step 2 — Account setup (15-30 min, you do alone)

These need your identity or payment method:

### 2a. GPG signing key
```bash
gpg --full-generate-key            # choose RSA 4096; expires in 2 years
gpg --list-keys --keyid-format=long
# Note the key ID (e.g., AB12CD34EF567890)
gpg --armor --export AB12CD34EF567890 > public.asc
gpg --armor --export-secret-keys AB12CD34EF567890 > private.asc   # for GHA secret
gpg --fingerprint AB12CD34EF567890
# Copy the 40-hex-char fingerprint string → that's the value for Step 1.3
```

### 2b. crates.io API token
- Log in at https://crates.io/me (uses GitHub OAuth)
- Account → API Tokens → New Token
- Name: `oz-policy-builder-release`
- Scopes: at minimum `publish-update` (recommended: also `publish-new`)
- Save the token string — you'll use it for `CARGO_REGISTRY_TOKEN`

### 2c. npm token
```bash
npm login
npm token create --read-only=false
# Save the token — you'll use it for NPM_TOKEN
```

### 2d. Fly.io account (or any container host)
- Sign up at https://fly.io (no card required for the trial)
- Add a payment method for sustained usage (small endpoint is ~$5-10/mo)
- Install the `flyctl` CLI: `brew install flyctl` or `curl -L https://fly.io/install.sh | sh`
- `fly auth login`

---

## Step 3 — Push code + set GHA secrets (10 min, you do)

Push the branch:
```bash
cd /Users/mert/Projects/oz-account-policy-builder/.worktrees/phase-1-foundations
git remote add origin https://github.com/<your-org>/oz-policy-builder.git
git push -u origin phase-1-foundations
```

Open the GitHub repo's **Settings → Secrets and variables → Actions** and add:

| Name | Value |
|---|---|
| `CARGO_REGISTRY_TOKEN` | From Step 2b |
| `NPM_TOKEN` | From Step 2c |
| `RELEASE_GPG_KEY` | Contents of `private.asc` from Step 2a (the full ASCII-armored block) |
| `OZ_POLICY_MCP_TOKEN` | A random 32-char hex string you generate (`openssl rand -hex 32`) |

Optional: merge `phase-1-foundations` → `main` via PR. The CI workflows (`ci.yml`, `walkthroughs.yml`, `reproducible-build.yml`) should all be green on the PR.

---

## Step 4 — Deploy hosted MCP endpoint (15-30 min, you do)

```bash
cd /Users/mert/Projects/oz-account-policy-builder/.worktrees/phase-1-foundations/infra/fly
# Edit fly.toml: set `app` to your chosen name (e.g., "oz-policy-yourname")
# Edit fly.toml: set `primary_region` to whatever's closest to you (e.g., "fra", "iad", "syd")
fly apps create <your-app-name>
fly secrets set OZ_POLICY_MCP_TOKEN=<same value from Step 3>
./deploy.sh
```

Verify:
```bash
curl https://<your-app-name>.fly.dev/healthz
# Expected: {"status":"ok","version":"0.0.0"}

# Drive the 5-tool MCP session:
curl -X POST https://<your-app-name>.fly.dev/mcp \
  -H "Authorization: Bearer <token>" \
  -H "Content-Type: application/json" \
  -d '{"jsonrpc":"2.0","id":1,"method":"tools/list"}'
# Expected: 5 tools returned
```

DNS (optional but recommended for branding):
- Point `mcp.your-domain.com` A record to your Fly IPv4 (`fly ips list`)
- `fly certs create mcp.your-domain.com` — Fly provisions a managed TLS cert

---

## Step 5 — External audit engagement (weeks–months, you initiate)

`audits/SCOPE.md` recommends **OtterSec** as the primary auditor. Five fallback firms are also SDF-blessed.

1. Email OtterSec: their public contact is on https://osec.io. Reference:
   - The pinned commit SHA on `phase-1-foundations`
   - `audits/handoff-package/README.md`
   - `audits/THREAT_MODEL.md`
   - `audits/SCOPE.md`
2. Negotiate scope + price. Typical for this codebase (~6 crates of synthesizer + templates + simhost + installer + wallet-adapter AuthPayload encoder): **$10-30k**, **1-2 week audit cycle**.
3. Run the engagement. When you have the report:
   - Commit it under `audits/<auditor>-<date>/report.pdf`
   - Open issues for each Critical / Important finding
   - I can cycle back to remediate (each fix is a normal PR cycle)

---

## Step 6 — Mainnet canary (30-60 min, you do)

Follow `docs/mainnet-readiness.md` step-by-step. Summary:

1. **Acquire ~5-10 mainnet XLM**. Options: DEX swap from another asset, CEX withdrawal to a fresh mainnet G-address, or community faucets (rare for mainnet).
2. **Generate mainnet keypair**: `stellar keys generate canary-mainnet --network mainnet`
3. **Fund it**: send 5+ XLM to the new G-address.
4. **Deploy a mainnet smart account**: same WASM upload/deploy flow as Phase 7's testnet path, but with `--network mainnet` and the funded keypair. Capture the C-address.
5. **Deploy a mainnet policy contract** (your choice: Phase 3 fixture, or a fresh codegen result against a real mainnet recording). Capture the C-address.
6. **Update `crates/oz-policy-installer/src/registry.rs`**: add the mainnet addresses to `project_deployed_policy_address`.
7. **Run the full pipeline**: `oz-policy-cli record --hash <mainnet-tx> --rpc <mainnet-rpc> --network "Public Global Stellar Network ; September 2015"` → synthesize → codegen → simulate → prepare-install. Sign with the canary keypair. Submit. Wait for confirmation.
8. **Verify**: call `verify_install` via the MCP server. Assert `matches: true`.
9. **Freeze the corpus**: create `walkthroughs/mainnet-canary/` with `deployed-addresses.json`, `tx-hash.txt`, `verify-report.json`, `README.md`. Commit.

If the install fails at any step, the failure is captured in real RPC error output — surface it and we debug together.

---

## Step 7 — v1.0.0 release (5 min, you do; GHA does the work)

After Steps 1-6 are complete:

```bash
# Update CHANGELOG.md: convert [Unreleased] section to [1.0.0] - YYYY-MM-DD
$EDITOR CHANGELOG.md
git add CHANGELOG.md && git commit -m "docs: CHANGELOG v1.0.0"

# Tag and push:
git tag -a v1.0.0 -m "v1.0.0: external audit complete + mainnet canary verified"
git push origin v1.0.0
```

The `release.yml` workflow runs automatically:
- Builds binaries for linux-amd64, linux-arm64, darwin-amd64, darwin-arm64
- Generates SHA256SUMS and signs with your GPG key
- Creates a GitHub Release with all artifacts attached
- Publishes all 7 crates to crates.io in dependency order (`oz-policy-core` first, `oz-policy-mcp` last)
- Publishes `@oz-policy-builder/wallet-adapter` to npm

Verify after the workflow completes:
- `cargo install oz-policy-cli` → works from any machine
- `pnpm add @oz-policy-builder/wallet-adapter` → installs cleanly
- `curl -L https://github.com/<org>/oz-policy-builder/releases/download/v1.0.0/SHA256SUMS` → readable
- `gpg --verify SHA256SUMS.asc SHA256SUMS` → "Good signature from <your GPG identity>"

---

## Reference docs (already in the repo)

- `STATUS.md` — phase-by-phase status snapshot
- `docs/mainnet-readiness.md` — full mainnet runbook (Step 6 expanded)
- `audits/READY.md` — pre-audit checklist (Step 5 prereqs)
- `audits/SCOPE.md` — what's in scope vs out of scope for audit
- `infra/README.md` — Fly.io deployment details (Step 4 expanded)
- `SECURITY.md` — disclosure policy (gets your email in Step 1)
- `CONTRIBUTING.md` — PR workflow + the 6 CI gates a PR must pass

---

## What I (Claude) can do once you give me Step 1's three values

I'll substitute them across the codebase in one commit, push to your remote (if you've added it), and confirm CI is green. Everything from Step 2 onward needs your identity / credentials / payment, which I deliberately cannot have.
