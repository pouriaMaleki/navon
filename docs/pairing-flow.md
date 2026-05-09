# Pairing flow — operator quick reference

This is the user-facing description of the QR-OOB pairing handshake the
device + companions go through on first boot. Engineering details and
the cross-platform JSON wire format live in
[ble-route-sync-contract.md](ble-route-sync-contract.md).

## First power-on
1. Power on the ESP32 bike minimap. The panel briefly shows the boot
   logo, then a QR code centered on a dark background.
2. Open the companion app on your phone. The home-screen device chip
   says **"Pair a new device"** — tap it.
3. Step 1 is an instructions card; tap **Open camera**. The phone asks
   for camera permission the first time; grant it.
4. Step 2 opens the camera. Hold the phone ~30 cm from the device's
   panel. The QR scans automatically and the screen flips to
   **Connecting…**, then **Confirming…**.
5. Step 3 shows **Paired!** for ~1.5s and the screen auto-dismisses
   back to home. The home chip now shows the device name with a
   connected accent — you're done.

If the camera doesn't scan after a few seconds, the screen surfaces a
*"Center the code in the frame"* hint. If pairing fails (ran out of
range, BLE disabled mid-flow, etc.) an alert lets you **Try again** or
**Cancel**.

## Re-pair (forget + bond again)
The device follows a strict **single-bond policy**: once it's paired
to one phone, it rejects pairing-confirm writes from any other phone
until the existing bond is forgotten.

To re-pair:
1. On the bonded companion, open **Settings → Device** and tap
   **Forget paired device**, then confirm in the alert.
2. The companion drops its locally-stored bond. The next time the
   device tries to write to the encrypted characteristic, the SMP
   exchange fails (no matching IRK on the phone side); the firmware
   sees the auth failure, drops `peer_identity` from NVS, and re-
   enters pairing mode.
3. Power-cycle the device or wait ~30 seconds — a fresh QR appears on
   the panel.
4. Run through the pairing flow on the new phone (or the same one) as
   above.

## Lost phone — what now?
Today the device's bond can only be cleared *from the bonded phone*.
If the phone is lost, the only recovery path is re-flashing the
device's NVS partition (a full firmware reflash via `espflash` does
this — see the bring-up doc in
[ble-route-sync-contract.md](ble-route-sync-contract.md#esp32-c6-slave-firmware)).

A factory-reset gesture on the device itself (long-press combo, button
sequence) is a planned follow-up but **not in this release** — see


## Anti-replay protections
- The QR's secret rotates every 60 seconds while the device is
  unbonded, so a stale photo can't be used to pair later.
- The first encrypted write to any of the three encrypted
  characteristics triggers SMP Just Works pairing; until SMP succeeds
  the BLE link isn't encrypted and routes can't flow.
- Once bonded, the device's advertising filter switches to
  `whitelist-only`; only the bonded phone's BD_ADDR can even *scan*
  the device after that.

## Operator notes for SMP

If pairing is failing with `auth_cmpl FAILURE — fail_reason=…` on the
device serial console, the most common causes are:

1. **C6 slave firmware out of date.** The boot log says something
   like `transport: Version mismatch: Host [2.12.0] > Co-proc [0.0.0]`.
   Build the matching slave image with `cargo xtask build-c6-slave`
   and flash it on the C6's UART connector
   (`espflash write-bin --chip esp32c6 --port <PORT> 0x0
   .xtask/c6-slave/c6-slave-merged.bin`). Reflashing the C6 wipes
   *its* NVS, but the P4's NVS where our bond lives is on a separate
   chip — `device_paired` survives.
2. **Stale iOS Settings → Bluetooth bond.** If a previous attempt
   succeeded at the SMP layer and the phone still has it in
   *Settings → Bluetooth*, *Forget This Device* there before
   re-pairing.
3. **Reflashing the P4** wipes its NVS, which means the bond is
   gone from the device side but still present on the phone — *Forget
   This Device* before re-pair.

## Diagnostics
The companion's settings → diagnostics card surfaces:
- the count of route-sync chunks the device dropped because it was
  still in pairing mode (a non-zero value here means a misbehaving
  companion is trying to push routes before bonding completes)
- the firmware version pulled out of the QR's `fw` field (helps spot
  a mismatch when the user is on an older companion)
- last connection-state transition + reason
