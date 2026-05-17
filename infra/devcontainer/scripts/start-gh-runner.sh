#!/usr/bin/env bash
# Start the GitHub Actions runner as a background daemon.
# Safe to run on rebuilds — stops any existing runner first.

set -euo pipefail

RUNNER_DIR="${HOME}/actions-runner"

if [ ! -f "${RUNNER_DIR}/.runner" ]; then
  echo "Runner not configured. Run install-gh-runner.sh <TOKEN> first."
  exit 1
fi

# Stop any existing runner process
if [ -f "${RUNNER_DIR}/.run.pid" ]; then
  old_pid=$(cat "${RUNNER_DIR}/.run.pid")
  if kill -0 "${old_pid}" 2>/dev/null; then
    echo "Stopping existing runner (pid ${old_pid}) ..."
    kill "${old_pid}" 2>/dev/null || true
    sleep 1
  fi
fi

echo "Starting runner ..."
cd "${RUNNER_DIR}"
nohup "${RUNNER_DIR}/run.sh" > "${RUNNER_DIR}/.run.log" 2>&1 &
echo $! > "${RUNNER_DIR}/.run.pid"
echo "Runner started (pid $!). Log: ${RUNNER_DIR}/.run.log"
