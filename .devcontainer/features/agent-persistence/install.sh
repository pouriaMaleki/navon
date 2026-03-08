#!/usr/bin/env bash
set -euo pipefail

REMOTE_USER="${_REMOTE_USER:-vscode}"
USER_HOME="$(getent passwd "${REMOTE_USER}" | cut -d: -f6 || true)"

if [ -z "${USER_HOME}" ]; then
  USER_HOME="/home/${REMOTE_USER}"
fi

link_mount() {
  local mount_path="$1"
  local link_path="$2"

  if [ ! -e "${mount_path}" ]; then
    return 0
  fi

  if [ -L "${link_path}" ] || [ -e "${link_path}" ]; then
    rm -rf "${link_path}"
  fi

  ln -s "${mount_path}" "${link_path}"
  chown -h "${REMOTE_USER}:${REMOTE_USER}" "${link_path}" || true
}

link_mount /mnt/.codex "${USER_HOME}/.codex"
link_mount /mnt/.claude "${USER_HOME}/.claude"
link_mount /mnt/.gemini "${USER_HOME}/.gemini"
