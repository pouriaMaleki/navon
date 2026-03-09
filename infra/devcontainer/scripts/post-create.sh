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

# In this devcontainer, ~/.cargo may be mounted as root-owned. Use writable CARGO_HOME.
if ! [ -w "${HOME}/.cargo" ]; then
  if ! grep -q 'export CARGO_HOME=/usr/local/cargo' "${HOME}/.bashrc"; then
    printf '\nexport CARGO_HOME=/usr/local/cargo\n' >> "${HOME}/.bashrc"
    printf 'export PATH="$CARGO_HOME/bin:$PATH"\n' >> "${HOME}/.bashrc"
  fi
  export CARGO_HOME=/usr/local/cargo
  export PATH="$CARGO_HOME/bin:$PATH"
fi

# Emulator toolchain dependency for `cargo xtask emu`.
if ! command -v wasm-pack >/dev/null 2>&1; then
  CARGO_HOME="${CARGO_HOME:-/usr/local/cargo}" cargo install wasm-pack
fi

# Keep local bun usable in future shells when present.
if [ -x "${HOME}/.bun/bin/bun" ] && ! grep -q 'export PATH="$HOME/.bun/bin:$PATH"' "${HOME}/.bashrc"; then
  printf '\nexport PATH="$HOME/.bun/bin:$PATH"\n' >> "${HOME}/.bashrc"
fi

# Keep git global config in a persistent mounted directory.
mkdir -p "${HOME}/.config/git"
if [ ! -f "${HOME}/.config/git/config" ]; then
  touch "${HOME}/.config/git/config"
fi
if [ -f "${HOME}/.gitconfig" ] && [ ! -s "${HOME}/.config/git/config" ]; then
  cp "${HOME}/.gitconfig" "${HOME}/.config/git/config"
fi
