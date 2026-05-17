#!/usr/bin/env bash
set -euo pipefail

cd /opt/actions-runner

# Configure on first run with token
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
  echo "Runner configured."
fi

exec ./run.sh
