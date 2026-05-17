#!/usr/bin/env bash
set -euo pipefail

CONFIG_DIR="/opt/actions-runner/_config"
mkdir -p "${CONFIG_DIR}"

# Restore persisted config on rebuilds
for f in .runner .credentials .credentials_rsaparams; do
  if [ -f "${CONFIG_DIR}/${f}" ] && [ ! -f "/opt/actions-runner/${f}" ]; then
    cp "${CONFIG_DIR}/${f}" "/opt/actions-runner/${f}"
  fi
done

cd /opt/actions-runner

# Register on first run
if [ ! -f .runner ]; then
  if [ -z "${GITHUB_RUNNER_TOKEN:-}" ]; then
    echo "GITHUB_RUNNER_TOKEN not set. Get one at:"
    echo "  https://github.com/pouriaMaleki/navon/settings/actions/runners/new"
    exit 1
  fi
  ./config.sh \
    --url https://github.com/pouriaMaleki/navon \
    --token "${GITHUB_RUNNER_TOKEN}" \
    --name ci-runner \
    --labels self-hosted,Linux \
    --unattended \
    --work _work
fi

# Persist config for next rebuild
cp -f .runner .credentials .credentials_rsaparams "${CONFIG_DIR}/" 2>/dev/null || true

exec ./run.sh
