#!/usr/bin/env bash
# Renders navon-icon.svg to all required PNG sizes for Android, PWA, and web.
# Requires: librsvg2-bin (rsvg-convert)
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
SVG="$SCRIPT_DIR/navon-icon.svg"
ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
ANDROID_RES="$ROOT/companion-apps/android/app/src/main/res"

render() {
  local size="$1" out="$2"
  mkdir -p "$(dirname "$out")"
  rsvg-convert -w "$size" -h "$size" -o "$out" "$SVG"
  echo "  ${size}×${size} → $out"
}

# ── Android adaptive icon foreground (108dp per density) ──
for density in mdpi hdpi xhdpi xxhdpi xxxhdpi; do
  case $density in
    mdpi)    px=108 ;;
    hdpi)    px=162 ;;
    xhdpi)   px=216 ;;
    xxhdpi)  px=324 ;;
    xxxhdpi) px=432 ;;
  esac
  render "$px" "$ANDROID_RES/mipmap-$density/ic_launcher_foreground.png"
done

# ── Android legacy launcher icons ──
for density in mdpi hdpi xhdpi xxhdpi xxxhdpi; do
  case $density in
    mdpi)    px=48 ;;
    hdpi)    px=72 ;;
    xhdpi)   px=96 ;;
    xxhdpi)  px=144 ;;
    xxxhdpi) px=192 ;;
  esac
  render "$px" "$ANDROID_RES/mipmap-$density/ic_launcher.png"
done

# ── PWA icons ──
render 192 "$ROOT/companion-apps/web/public/icon-192.png"
render 512 "$ROOT/companion-apps/web/public/icon-512.png"

# ── iOS apple-touch-icon for homepage ──
render 180 "$ROOT/homepage/public/apple-touch-icon.png"

echo ""
echo "Done. All PNGs rendered."
