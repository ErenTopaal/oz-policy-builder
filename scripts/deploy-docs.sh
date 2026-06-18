#!/usr/bin/env bash
# Builds the Fumadocs static site under docs-site/ and deploys the out/
# directory to yokai at /var/www/policy-docs/.
#
# Assumes:
#   * `yokai` is configured in ~/.ssh/config and reaches the policy host.
#   * The user invoking this has sudo NOPASSWD for `rsync` into /var/www/policy-docs
#     and `systemctl reload caddy` on yokai.
#   * The Caddyfile already contains a `docs.policy.erentopal.xyz` block pointing
#     at /var/www/policy-docs.
#
# If the build dir is missing or rsync fails, exits non-zero and the live site
# is left untouched.

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
SRC_DIR="${REPO_ROOT}/docs-site"
OUT_DIR="${SRC_DIR}/out"
REMOTE_HOST="yokai"
REMOTE_STAGE="/tmp/policy-docs-new"
REMOTE_LIVE="/var/www/policy-docs"

echo "Building docs in ${SRC_DIR} ..."
( cd "$SRC_DIR" && pnpm build 2>&1 | tail -10 )

if [ ! -d "$OUT_DIR" ]; then
  echo "deploy-docs: $OUT_DIR not found after build" >&2
  exit 2
fi

echo "Syncing $OUT_DIR -> ${REMOTE_HOST}:${REMOTE_STAGE}/ ..."
rsync -avz --delete "${OUT_DIR}/" "${REMOTE_HOST}:${REMOTE_STAGE}/"

echo "Promoting ${REMOTE_STAGE} -> ${REMOTE_LIVE} on ${REMOTE_HOST} ..."
ssh "$REMOTE_HOST" "
  set -euo pipefail
  sudo rsync -a --delete '${REMOTE_STAGE}/' '${REMOTE_LIVE}/'
  sudo chown -R caddy:caddy '${REMOTE_LIVE}'
  rm -rf '${REMOTE_STAGE}'
"

echo "Deploy complete."
