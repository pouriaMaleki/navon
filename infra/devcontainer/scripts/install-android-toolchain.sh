#!/usr/bin/env bash
# Idempotent installer for the JDK + Android SDK pair the companion-android
# project needs. The official Android docs and AGP 8.13 require JDK 17+;
# we install Temurin 21 LTS because it's the latest "recommended" JDK
# Android still officially supports (AGP 8.7+ added JDK 21 support) and
# matches what large modern projects ship with.
#
# Tools land under /work/.tools so they live on the workspace bind mount
# (survives container rebuilds in this devcontainer setup) and don't
# duplicate inside the home volume. Future shells pick them up via the
# JAVA_HOME / ANDROID_HOME / PATH lines this script adds to ~/.bashrc.
#
# Verified against:
#   - Debian Bookworm + glibc
#   - JDK Temurin 21.0.11+10 (LTS)
#   - Android cmdline-tools 11076708
#   - platform-tools, platforms;android-35, build-tools;35.0.0
#
# Re-running is safe: each install step short-circuits if the artifact
# is already present.

set -euo pipefail

TOOLS_DIR="${TOOLS_DIR:-/work/.tools}"
JDK_DIR_NAME="jdk-21.0.11+10"
SDK_DIR_NAME="android-sdk"
JDK_DIR="${TOOLS_DIR}/${JDK_DIR_NAME}"
SDK_DIR="${TOOLS_DIR}/${SDK_DIR_NAME}"

mkdir -p "${TOOLS_DIR}"

if [ ! -d "${JDK_DIR}" ]; then
  echo "Installing Temurin JDK 21 LTS to ${JDK_DIR} ..."
  TMP_JDK="${TOOLS_DIR}/jdk21.tar.gz"
  curl -sL -o "${TMP_JDK}" \
    "https://api.adoptium.net/v3/binary/latest/21/ga/linux/x64/jdk/hotspot/normal/eclipse"
  tar -C "${TOOLS_DIR}" -xzf "${TMP_JDK}"
  rm -f "${TMP_JDK}"
fi

if [ ! -x "${SDK_DIR}/cmdline-tools/latest/bin/sdkmanager" ]; then
  echo "Installing Android cmdline-tools to ${SDK_DIR} ..."
  mkdir -p "${SDK_DIR}/cmdline-tools"
  TMP_CLI="${SDK_DIR}/cmdline-tools.zip"
  curl -sL -o "${TMP_CLI}" \
    "https://dl.google.com/android/repository/commandlinetools-linux-11076708_latest.zip"
  unzip -q "${TMP_CLI}" -d "${SDK_DIR}/cmdline-tools"
  rm -f "${TMP_CLI}"
  mv "${SDK_DIR}/cmdline-tools/cmdline-tools" "${SDK_DIR}/cmdline-tools/latest"
fi

export JAVA_HOME="${JDK_DIR}"
export ANDROID_HOME="${SDK_DIR}"
export PATH="${JAVA_HOME}/bin:${ANDROID_HOME}/cmdline-tools/latest/bin:${ANDROID_HOME}/platform-tools:${PATH}"

# `licenses` writes ack files; the loop is "yes | sdkmanager --licenses".
# Running with --licenses is idempotent — already-accepted licenses are
# no-ops.
yes 2>/dev/null | sdkmanager --licenses > /dev/null || true

# platforms;android-35 + build-tools;35.0.0 match libs.versions.toml
# (androidCompileSdk = androidTargetSdk = 35).
sdkmanager \
  "platform-tools" \
  "platforms;android-35" \
  "build-tools;35.0.0"

# Ensure future shells find both toolchains.
BASHRC="${HOME}/.bashrc"
SENTINEL="# >>> android-toolchain (managed by infra/devcontainer/scripts/install-android-toolchain.sh) >>>"
if ! grep -qF "${SENTINEL}" "${BASHRC}" 2>/dev/null; then
  {
    printf '\n%s\n' "${SENTINEL}"
    printf 'export JAVA_HOME=%q\n' "${JDK_DIR}"
    printf 'export ANDROID_HOME=%q\n' "${SDK_DIR}"
    printf 'export PATH="${JAVA_HOME}/bin:${ANDROID_HOME}/cmdline-tools/latest/bin:${ANDROID_HOME}/platform-tools:${PATH}"\n'
    printf '# <<< android-toolchain <<<\n'
  } >> "${BASHRC}"
fi

echo
echo "Done. Restart your shell or:"
echo "  export JAVA_HOME=${JDK_DIR}"
echo "  export ANDROID_HOME=${SDK_DIR}"
echo "  export PATH=\$JAVA_HOME/bin:\$ANDROID_HOME/cmdline-tools/latest/bin:\$ANDROID_HOME/platform-tools:\$PATH"
echo
echo "Verify with:"
echo "  cd /work/companion-android && ./gradlew :app:assembleDebug"
