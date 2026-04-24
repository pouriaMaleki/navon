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

4. Flash + monitor:

   ```
   espflash flash --chip esp32p4 --monitor --port COM3 firmware-release.bin
   ```

   Expected boot output on the serial monitor:

   ```
   I (xxx) boot: ESP-IDF v5.4.2 2nd stage bootloader
   ...
   frame=0 dt_us=~16000 lit_pixels=0 geometry=0
   frame=60 dt_us=~16000 lit_pixels=0 geometry=0
   ...
   ```

   (At this stage — Phase 2 — the firmware runs headless without a panel or
   GPS. The frame counter proves the toolchain end-to-end.)

## Troubleshooting

- **"no serial port found"**: confirm the Waveshare board is in download mode.
  Hold the `BOOT` button while pressing `RESET`, release `RESET`, release
  `BOOT`. `espflash` will then see the USB-Serial-JTAG as a flash target.
- **"discards section"** link errors: verify `map-runtime`'s
  `embedded-city-map` feature is off in `firmware/Cargo.toml`. That feature
  embeds a 79 MB `city.svm` that exceeds the P4's 64 MB flash DROM region.
- **Build fails with `no module named venv`**: run
  `sudo apt-get install -y python3-venv` in the devcontainer. ESP-IDF's
  `idf_tools.py` needs venv to provision its Python environment.

## Status

- Phase 1 (runtime crates `no_std`): **done**.
- Phase 2 (device boot + serial heartbeat): **in progress**.
- Phase 3 (PSRAM framebuffer + MIPI-DSI panel + real map loader from flash
  partition): not started.
- Phase 4 (migrate to `esp-hal` when P4 support lands): blocked on upstream.
  Run `cargo info esp-hal | grep esp32p4` to check.

The `firmware-idf-legacy` branch preserves the pre-`no_std`-port attempt at
device bring-up if you need to diff.
