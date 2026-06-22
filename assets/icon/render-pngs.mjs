// Renders navon-icon.svg to all required PNG sizes for Android, PWA, and web.
// Usage: node render-pngs.mjs
// Requires: npx @resvg/resvg-js (auto-fetched, no install needed)

import { readFileSync, writeFileSync, mkdirSync } from 'node:fs';
import { join, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';
import { Resvg } from '@resvg/resvg-js';

const __dirname = dirname(fileURLToPath(import.meta.url));
const ROOT = join(__dirname, '..', '..');
const SVG_PATH = join(__dirname, 'navon-icon.svg');
const svgSource = readFileSync(SVG_PATH);

function render(size, outPath) {
  const resvg = new Resvg(svgSource, {
    fitTo: { mode: 'width', value: size },
  });
  const png = resvg.render().asPng();
  mkdirSync(dirname(outPath), { recursive: true });
  writeFileSync(outPath, png);
  console.log(`  ${size}×${size} → ${outPath}`);
}

// ── Android adaptive icon foreground (108dp × 108dp per density) ──
const ANDROID_RES = join(ROOT, 'companion-apps', 'android', 'app', 'src', 'main', 'res');
const ADAPTIVE_SIZES = { mdpi: 108, hdpi: 162, xhdpi: 216, xxhdpi: 324, xxxhdpi: 432 };
for (const [density, px] of Object.entries(ADAPTIVE_SIZES)) {
  render(px, join(ANDROID_RES, `mipmap-${density}`, 'ic_launcher_foreground.png'));
}

// ── Android legacy launcher icon ──
const LEGACY_SIZES = { mdpi: 48, hdpi: 72, xhdpi: 96, xxhdpi: 144, xxxhdpi: 192 };
for (const [density, px] of Object.entries(LEGACY_SIZES)) {
  render(px, join(ANDROID_RES, `mipmap-${density}`, 'ic_launcher.png'));
}

// ── PWA icons ──
const WEB_PUBLIC = join(ROOT, 'companion-apps', 'web', 'public');
render(192, join(WEB_PUBLIC, 'icon-192.png'));
render(512, join(WEB_PUBLIC, 'icon-512.png'));

// ── iOS apple-touch-icon for homepage ──
const HOMEPAGE_PUBLIC = join(ROOT, 'homepage', 'public');
render(180, join(HOMEPAGE_PUBLIC, 'apple-touch-icon.png'));

console.log('\nDone. All PNGs rendered.');
