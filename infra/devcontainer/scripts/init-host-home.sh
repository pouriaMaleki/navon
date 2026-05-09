#!/usr/bin/env bash
set -euo pipefail

HOST_HOME="${HOME:?HOME is required}"
PERSIST_HOME="${HOST_HOME}/.devcontainer-homes/esp32-map"
MIGRATION_MARKER="${PERSIST_HOME}/.migration_v2_done"

mkdir -p "${HOST_HOME}/.ssh"
mkdir -p "${PERSIST_HOME}"

if [ -f "${MIGRATION_MARKER}" ]; then
  exit 0
fi

migrate_dir_once() {
  local dir_name="$1"
  local source_dir="${HOST_HOME}/${dir_name}"
  local target_dir="${PERSIST_HOME}/${dir_name}"

  if [ ! -e "${target_dir}" ] && [ -e "${source_dir}" ]; then
    mkdir -p "$(dirname "${target_dir}")"
    cp -a "${source_dir}" "${target_dir}"
  fi
}

migrate_dir_once ".codex"
migrate_dir_once ".claude"
migrate_dir_once ".gemini"
migrate_dir_once ".config/gh"
date -u +"%Y-%m-%dT%H:%M:%SZ" > "${MIGRATION_MARKER}"
