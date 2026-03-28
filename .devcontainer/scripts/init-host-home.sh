#!/usr/bin/env bash
set -euo pipefail

HOST_HOME="${HOME:?HOME is required}"
PERSIST_HOME="${HOST_HOME}/.devcontainer-homes/esp32-map"
MIGRATION_MARKER="${PERSIST_HOME}/.migration_v1_done"

mkdir -p "${PERSIST_HOME}"

if [ -f "${MIGRATION_MARKER}" ]; then
  exit 0
fi

migrate_dir_once() {
  local dir_name="$1"
  local source_dir="${HOST_HOME}/${dir_name}"
  local target_dir="${PERSIST_HOME}/${dir_name}"

  if [ ! -e "${target_dir}" ] && [ -e "${source_dir}" ]; then
    cp -a "${source_dir}" "${target_dir}"
  fi
}

migrate_dir_once ".codex"
migrate_dir_once ".claude"
migrate_dir_once ".gemini"

date -u +"%Y-%m-%dT%H:%M:%SZ" > "${MIGRATION_MARKER}"
