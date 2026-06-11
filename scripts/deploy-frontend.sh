#!/usr/bin/env bash
# Deploys the built frontend (dist/) to the yokai server.
#
# Critical: --exclude=sample-hash.txt and --exclude='preset-*.txt' keep the
# cron-managed preset files (refresh-sample-hash.timer + the three
# refresh-preset-*.timer units) from being wiped by --delete.
#
# Assumes:
#   * `yokai` is configured in ~/.ssh/config and reaches the policy host.
#   * The user invoking this has sudo NOPASSWD for `mv /tmp/policy-new /var/www/policy` on yokai.
#   * `vite build` (or equivalent) has produced dist/ locally.
#
# Honest source: if the build dir is missing or rsync fails, this script exits
# non-zero and the live site is left untouched. No fake successful exit.

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
DIST_DIR="${REPO_ROOT}/frontend/dist"
REMOTE_HOST="yokai"
REMOTE_STAGE="/tmp/policy-new"
REMOTE_LIVE="/var/www/policy"

if [ ! -d "$DIST_DIR" ]; then
  echo "deploy-frontend: $DIST_DIR not found — run \`npm run build\` first" >&2
  exit 2
fi

echo "Syncing $DIST_DIR -> ${REMOTE_HOST}:${REMOTE_STAGE}/ ..."
rsync -avz --delete \
  --exclude='sample-hash.txt' \
  --exclude='preset-*.txt' \
  "${DIST_DIR}/" "${REMOTE_HOST}:${REMOTE_STAGE}/"

echo "Promoting ${REMOTE_STAGE} -> ${REMOTE_LIVE} on ${REMOTE_HOST} ..."
ssh "$REMOTE_HOST" "
  set -euo pipefail
  # Preserve the cron-managed text files across the swap so the playground's
  # preset dropdown doesn't blip to 404 between deploys.
  for f in sample-hash.txt preset-blend.txt preset-sep41.txt preset-soroswap.txt; do
    if [ -f '${REMOTE_LIVE}/'\$f ]; then
      sudo cp -a '${REMOTE_LIVE}/'\$f '${REMOTE_STAGE}/'\$f
    fi
  done
  sudo rsync -a --delete \
    --exclude='sample-hash.txt' \
    --exclude='preset-*.txt' \
    '${REMOTE_STAGE}/' '${REMOTE_LIVE}/'
  sudo chown -R caddy:caddy '${REMOTE_LIVE}'
  rm -rf '${REMOTE_STAGE}'
"

echo "Deploy complete."
