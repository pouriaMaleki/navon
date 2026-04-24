# firmware — ESP32-P4 device crate

This crate targets the Waveshare ESP32-P4-Module-DEV-KIT via `esp-idf-svc`.

## Build (in the devcontainer)

```
cargo xtask bundle-device           # release build, merged .bin
cargo xtask bundle-device --debug   # debug build (larger, slower)
```

Output lands at `.xtask/device/firmware-release.bin` (or `firmware-debug.bin`).
The first-run build fetches ESP-IDF and compiles its C components; later builds
reuse the `/work/target/.embuild/espressif/` cache.

## Flash from a Windows laptop

The devcontainer is host-only (no USB passthrough). Flash from whichever
machine the P4's USB is plugged into.

**One-time setup on the Windows laptop:**

1. Install Rust: <https://www.rust-lang.org/tools/install> (MSRV 1.88).
2. `cargo install espflash --locked`
3. Install the USB-Serial driver if Windows doesn't auto-detect the board
   (Waveshare P4-Module-DEV-KIT uses the built-in USB-Serial-JTAG; usually
   works out of the box on Windows 10+).

**Per-flash iteration:**

1. Build in the devcontainer: `cargo xtask bundle-device`
2. Copy the `.bin` to the laptop (shared folder, `scp`, or `\\tsclient` in RDP):

   ```
   scp thinkpad:/work/.xtask/device/firmware-release.bin .
   ```

3. Plug the P4 in via USB-C. Find the COM port (Device Manager → Ports). Expect
   something like `COM3` / `COM4`.

4. Flash the merged image (starts at offset 0x0, contains bootloader +
   partition table + factory app):

   ```
   espflash write-bin --chip esp32p4 --port COM3 0x0 firmware-release.bin
   espflash monitor --chip esp32p4 --port COM3
   ```

   Expected serial output after power-on or `CTRL+R` reset:

   ```
   I (xxx) boot: ESP-IDF v5.4.2 2nd stage bootloader
   ...
   I (xxx) esp_psram: Speed: 200MHz
   I (xxx) esp_psram: Adding pool of 32768K of PSRAM memory to heap allocator
   ...
   I (xxx) firmware::esp_idf: esp32p4 heap: internal_free=... KB, psram_free=... KB
   I (xxx) firmware::esp_idf: esp32p4 bring-up: viewport=800x800, frame_interval_ms=16
   I (xxx) firmware::esp_idf: frame=N fps=<X> avg_work_ms=<Y> geometry=<N>
   ```

   `geometry > 0` proves the embedded `city-small.svm` loaded and the map
   is being queried each frame. The firmware is still headless (no panel,
   no GPS, no touch) — the next bring-up step wires those.

## Map data regeneration

The firmware embeds `/work/map-data/city-small.svm` (~9.6 MB, 400k segments
from the top road-class tiers) — a downsample of the full 76 MB
`/work/map-data/city.svm` used by the emulator. The small variant fits in
the P4's 64 MB flash-mapped DROM region.

Regenerate whenever the full map changes:

```bash
cargo run -p map-vector-cli --release -- \
  shrink-svm --input map-data/city.svm \
            --output map-data/city-small.svm \
            --max-segments 400000
```

## Troubleshooting

- **"no serial port found"**: confirm the Waveshare board is in download mode.
  Hold the `BOOT` button while pressing `RESET`, release `RESET`, release
  `BOOT`. `espflash` will then see the USB-Serial-JTAG as a flash target.
- **"discards section"** link errors: the `map-runtime` feature selection
  uses `cfg(target_os = "espidf")` so the full 76 MB `city.svm` is never
  embedded on device. If you see this, check that no workspace member
  overrides `map-runtime`'s feature set in a way that enables
  `embedded-map` on a non-espidf path.
- **"Supplied ELF image … too big"**: `CONFIG_PARTITION_TABLE_CUSTOM`
  must point at `firmware/partitions.csv` (via the absolute path in
  `sdkconfig.defaults`) and `CONFIG_ESPTOOLPY_FLASHSIZE_32MB=y` must be set.
  `cargo xtask bundle-device` passes matching `--partition-table` and
  `--flash-size 32mb` flags to `espflash save-image`.
- **Build fails with `no module named venv`**: run
  `sudo apt-get install -y python3-venv` in the devcontainer. ESP-IDF's
  `idf_tools.py` needs venv to provision its Python environment.

## Status

- Phase 1 (runtime crates `no_std`): **done**.
- Phase 2 (device boot + serial heartbeat): **done**.
- Phase 3 (PSRAM at 200 MHz, framebuffer in PSRAM, MIPI-DSI scaffolded):
  **partial** — panel driver wiring depends on confirmed Waveshare SKU.
- **Embedded map**: **done** — `city-small.svm` (400k segments) ships
  inside the firmware binary; `geometry > 0` on boot.
- Phase 4 (migrate to `esp-hal` when P4 support lands): blocked on upstream.
  Run `cargo xtask check-esp-hal-p4` to probe.

The `firmware-idf-legacy` branch preserves the pre-`no_std`-port attempt at
device bring-up if you need to diff.
