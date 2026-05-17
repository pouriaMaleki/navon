#!/usr/bin/env bash
# Idempotent installer for the GitHub Actions self-hosted runner.
# Binary + config live in ~/actions-runner (persistent home volume).
# Re-running is safe: downloads only if missing, configures only once.
#
# First run (no token cached):
#   install-gh-runner.sh <TOKEN>
# Rebuilds (already configured):
#   install-gh-runner.sh
#
# The runner is NOT started here; use start-gh-runner.sh or the
# supervisor in post-create.sh.

set -euo pipefail

RUNNER_DIR="${HOME}/actions-runner"
RUNNER_VERSION="2.334.0"
RUNNER_TARBALL="actions-runner-linux-x64-${RUNNER_VERSION}.tar.gz"
RUNNER_URL="https://github.com/actions/runner/releases/download/v${RUNNER_VERSION}/${RUNNER_TARBALL}"

mkdir -p "${RUNNER_DIR}"

# ── Download ──────────────────────────────────────────────────────────
if [ ! -f "${RUNNER_DIR}/config.sh" ]; then
  echo "Downloading GitHub Actions runner ${RUNNER_VERSION} ..."
  curl -sL -o "${RUNNER_DIR}/${RUNNER_TARBALL}" "${RUNNER_URL}"
  tar xzf "${RUNNER_DIR}/${RUNNER_TARBALL}" -C "${RUNNER_DIR}"
  rm -f "${RUNNER_DIR}/${RUNNER_TARBALL}"
fi

# ── Configure (only on first run with token) ──────────────────────────
TOKEN="${1:-}"
if [ -n "${TOKEN}" ] && [ ! -f "${RUNNER_DIR}/.runner" ]; then
  echo "Configuring runner ..."
  "${RUNNER_DIR}/config.sh" \
    --url https://github.com/pouriaMaleki/navon \
    --token "${TOKEN}" \
    --name devcontainer \
    --labels self-hosted,Linux \
    --unattended \
    --work "${RUNNER_DIR}/_work"
  echo "Runner configured."
elif [ -z "${TOKEN}" ] && [ ! -f "${RUNNER_DIR}/.runner" ]; then
  echo "Runner not configured. Run this script with a token:"
  echo "  ${0} <GITHUB_RUNNER_TOKEN>"
  echo "Get a token at: https://github.com/pouriaMaleki/navon/settings/actions/runners/new"
  exit 1
else
  echo "Runner already configured."
fi
