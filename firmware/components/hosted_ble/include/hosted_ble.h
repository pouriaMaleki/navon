// Bring-up of the BLE host stack on the ESP32-P4 via the on-board ESP32-C6.
//
// The P4 has no on-chip radio. Waveshare wires an ESP32-C6 to the P4's
// SDIO port, and `espressif/esp_hosted` runs the controller side on the C6
// while exposing a virtual HCI on the P4. This component:
//
//   1. releases controller memory we don't need (Classic BT),
//   2. calls esp_bt_controller_init/enable for the local "remote" controller,
//   3. opens the hosted HCI transport,
//   4. attaches it to Bluedroid via esp_bluedroid_attach_hci_driver,
//   5. calls esp_bluedroid_init/enable so the GATT/GAP APIs are usable.
//
// After hosted_ble_init() returns ESP_OK, the standard
// `esp_ble_gatts_*` / `esp_ble_gap_*` APIs work as if we had on-chip radio.
//
// hosted_ble_init() is idempotent — calling it twice is safe and the
// second call returns ESP_OK without re-doing the bring-up.
//
// hosted_ble_route_sync_start() builds the fixed route-sync GATT service
// (UUIDs match `docs/ble-route-sync-contract.md`) and begins advertising.
// Inbound chunk writes are delivered to the registered callback verbatim;
// the Rust side decodes the BLE packet envelope and reassembles the route
// transfer. hosted_ble_route_sync_notify() pushes one outbound packet on
// the event characteristic when a companion is connected.

#pragma once

#include "esp_err.h"

#include <stdbool.h>
#include <stddef.h>
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

esp_err_t hosted_ble_init(void);

typedef void (*hosted_ble_chunk_cb_t)(const uint8_t *data, size_t len, void *ctx);

// Called when a companion writes the 32-byte ephemeral secret to the
// pairing-confirm characteristic (UUID …-1004). The Rust side matches
// it against the current QR's secret in `pairing.rs` and transitions
// the device from Pairing → Operational on a match.
typedef void (*hosted_ble_pairing_confirm_cb_t)(const uint8_t *data, size_t len, void *ctx);

// Tells the C side whether to reject pairing-confirm writes outright
// (single-bond policy: once a bond is stored, the user must Forget
// from the companion before re-pairing). Returning `true` accepts the
// write and forwards it to `on_pairing_confirm`; `false` rejects with
// `ESP_GATT_WRITE_NOT_PERMIT`.
typedef bool (*hosted_ble_is_pairing_mode_cb_t)(void *ctx);

typedef struct {
    hosted_ble_chunk_cb_t on_chunk;
    hosted_ble_pairing_confirm_cb_t on_pairing_confirm;
    hosted_ble_is_pairing_mode_cb_t is_pairing_mode;
    void *ctx;
} hosted_ble_route_sync_callbacks_t;

esp_err_t hosted_ble_route_sync_start(const hosted_ble_route_sync_callbacks_t *cb);

esp_err_t hosted_ble_route_sync_notify(const uint8_t *data, size_t len);

// Switch the advertising filter policy. Called by the runtime once the
// device transitions to `Operational` so subsequent advertising windows
// only accept the bonded peer in the controller's whitelist. The
// `peer_addr` (BD_ADDR) is added to the whitelist by this call when
// `whitelist_only == true`.
//
// Mirror of `AdvertisingFilterPolicy` in `hosted_ble_state.rs`:
//   whitelist_only == false → ADV_FILTER_ALLOW_SCAN_ANY_CON_ANY
//   whitelist_only == true  → ADV_FILTER_ALLOW_SCAN_WLST_CON_WLST
esp_err_t hosted_ble_route_sync_set_adv_filter(bool whitelist_only,
                                               const uint8_t peer_addr[6],
                                               uint8_t addr_type);

#ifdef __cplusplus
}
#endif
