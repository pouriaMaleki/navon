#!/usr/bin/env bash
# Start the GitHub Actions runner as a background daemon.
# Safe to run on rebuilds — stops any existing runner first.

set -euo pipefail

RUNNER_DIR="${HOME}/actions-runner"

if [ ! -f "${RUNNER_DIR}/.runner" ]; then
  echo "Runner not configured. Run install-gh-runner.sh <TOKEN> first."
  exit 1
fi

# Stop any existing Runner.Listener processes
pkill -f "Runner.Listener" 2>/dev/null || true
sleep 1

echo "Starting runner ..."
nohup "${RUNNER_DIR}/run.sh" > "${RUNNER_DIR}/.run.log" 2>&1 &
echo "Runner started (pid $!). Log: ${RUNNER_DIR}/.run.log"
