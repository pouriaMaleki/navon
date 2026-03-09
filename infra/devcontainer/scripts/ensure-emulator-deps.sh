#!/usr/bin/env bash
set -euo pipefail

detect_package_manager() {
  if command -v npm >/dev/null 2>&1; then
    command -v npm
    return 0
  fi
  if command -v pnpm >/dev/null 2>&1; then
    command -v pnpm
    return 0
  fi
  if command -v bun >/dev/null 2>&1; then
    command -v bun
    return 0
  fi
  if [ -x "${HOME}/.bun/bin/bun" ]; then
    echo "${HOME}/.bun/bin/bun"
    return 0
  fi
  return 1
}

persist_bun_path() {
  if [ -x "${HOME}/.bun/bin/bun" ] && ! grep -q 'export PATH="$HOME/.bun/bin:$PATH"' "${HOME}/.bashrc"; then
    printf '\nexport PATH="$HOME/.bun/bin:$PATH"\n' >> "${HOME}/.bashrc"
  fi
}

if ! command -v wasm-pack >/dev/null 2>&1; then
  echo "Installing wasm-pack..."
  CARGO_HOME="${CARGO_HOME:-/usr/local/cargo}" cargo install wasm-pack
else
  echo "wasm-pack already installed"
fi

if pm="$(detect_package_manager)"; then
  echo "JS package manager detected: ${pm}"
  persist_bun_path
  exit 0
fi

echo "No JS package manager found; attempting local bun bootstrap..."
if ! command -v curl >/dev/null 2>&1; then
  echo "curl is missing."
  echo "Install Node.js/npm (recommended) or bun manually:"
  echo "  - Debian/Ubuntu: sudo apt-get update && sudo apt-get install -y nodejs npm"
  echo "  - Bun: curl -fsSL https://bun.sh/install | bash"
  exit 1
fi

export BUN_INSTALL="${HOME}/.bun"
curl -fsSL https://bun.sh/install | bash
persist_bun_path

if pm="$(detect_package_manager)"; then
  echo "JS package manager detected: ${pm}"
  exit 0
fi

echo "No JS package manager found after bun bootstrap."
echo "Install Node.js/npm (recommended) or bun manually:"
echo "  - Debian/Ubuntu: sudo apt-get update && sudo apt-get install -y nodejs npm"
echo "  - Bun: curl -fsSL https://bun.sh/install | bash"
echo "If bun is installed at ~/.bun/bin, add it to PATH:"
echo '  export PATH="$HOME/.bun/bin:$PATH"'
exit 1
