#!/usr/bin/env bash
set -euo pipefail

# Install Espressif toolchain once per user if not already present.
if [ ! -f "${HOME}/export-esp.sh" ]; then
  espup install
fi

# Ensure shell sessions load ESP exports when available.
if [ -f "${HOME}/export-esp.sh" ] && ! grep -q 'source "$HOME/export-esp.sh"' "${HOME}/.bashrc"; then
  printf '\nsource "$HOME/export-esp.sh"\n' >> "${HOME}/.bashrc"
fi
