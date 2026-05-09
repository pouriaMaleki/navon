// Route-sync GATT server.
//
// Service / characteristic UUIDs mirror `docs/ble-route-sync-contract.md`:
//
//   service    8d0f3f30-7b4d-4f7c-8b24-2f8e7e4e1001
//   chunk_w    8d0f3f30-7b4d-4f7c-8b24-2f8e7e4e1002  (write / write-no-rsp)
//   event_n    8d0f3f30-7b4d-4f7c-8b24-2f8e7e4e1003  (notify)
//   pair_w     8d0f3f30-7b4d-4f7c-8b24-2f8e7e4e1004  (write, encrypted)
//
// The companion (iOS / Android) writes BLE packets containing
// `RouteTransferChunk` payloads to the chunk characteristic. The Rust side
// reassembles them, decodes the canonical RouteSyncMessage, and runs the
// route through the same RouteSyncTransport the firmware tests cover.
// Outbound `status` and `reroute_request` messages travel back through
// hosted_ble_route_sync_notify() on the notify characteristic.
//
// This file uses the raw esp_ble_gatts_* / esp_ble_gap_* C API rather than
// esp-idf-svc's BtDriver wrapper because the P4 has no on-chip modem
// peripheral — the controller runs on the ESP32-C6 over the hosted HCI
// transport set up by hosted_ble_init() (see hosted_ble.c).

#include "hosted_ble.h"

#include <inttypes.h>
#include <stdatomic.h>
#include <stdbool.h>
#include <string.h>

#include "esp_bt_defs.h"
#include "esp_bt_main.h"
#include "esp_gap_ble_api.h"
#include "esp_gatt_common_api.h"
#include "esp_gatt_defs.h"
#include "esp_gatts_api.h"
#include "esp_log.h"

static const char *TAG = "hosted_ble_rs";

#define HOSTED_BLE_APP_ID 0x4553
#define HOSTED_BLE_DEVICE_NAME "ESP32 Bike Minimap"
// Each characteristic adds its declaration + value handle; the event
// notify char also needs a CCCD. With four characteristics (chunk +
// event-with-CCCD + pairing-confirm + pairing-request) we need 4*2 + 1
// = 9 handles for attributes plus the service handle = 10. 14 leaves a
// small cushion for attribute reservation. Under-provisioning causes
// ESP_GATT_NO_RESOURCES on `esp_ble_gatts_add_char` after the cap.
#define HOSTED_BLE_SERVICE_NUM_HANDLE 16
#define HOSTED_BLE_MAX_PACKET_LEN 512
// Pairing-confirm payload is exactly the 32-byte ephemeral secret from
// the QR. Anything longer is malformed; reject early to keep the C path
// simple.
#define HOSTED_BLE_PAIRING_SECRET_LEN 32

// 128-bit UUIDs are sent on the wire little-endian, so flip the readable
// big-endian form `8d0f3f30-7b4d-4f7c-8b24-2f8e7e4e100X` byte-for-byte.
static const uint8_t SERVICE_UUID128[ESP_UUID_LEN_128] = {
    0x01, 0x10, 0x4e, 0x7e, 0x8e, 0x2f, 0x24, 0x8b,
    0x7c, 0x4f, 0x4d, 0x7b, 0x30, 0x3f, 0x0f, 0x8d,
};
static const uint8_t CHUNK_UUID128[ESP_UUID_LEN_128] = {
    0x02, 0x10, 0x4e, 0x7e, 0x8e, 0x2f, 0x24, 0x8b,
    0x7c, 0x4f, 0x4d, 0x7b, 0x30, 0x3f, 0x0f, 0x8d,
};
static const uint8_t EVENT_UUID128[ESP_UUID_LEN_128] = {
    0x03, 0x10, 0x4e, 0x7e, 0x8e, 0x2f, 0x24, 0x8b,
    0x7c, 0x4f, 0x4d, 0x7b, 0x30, 0x3f, 0x0f, 0x8d,
};
static const uint8_t PAIRING_UUID128[ESP_UUID_LEN_128] = {
    0x04, 0x10, 0x4e, 0x7e, 0x8e, 0x2f, 0x24, 0x8b,
    0x7c, 0x4f, 0x4d, 0x7b, 0x30, 0x3f, 0x0f, 0x8d,
};
// Pairing-request characteristic (UUID …-1005). Unencrypted by design:
// the companion writes it before any SMP pairing has happened to ask
// the device to start showing its QR. The actual bond is still gated
// by the encrypted pairing-confirm characteristic above.
static const uint8_t PAIRING_REQUEST_UUID128[ESP_UUID_LEN_128] = {
    0x05, 0x10, 0x4e, 0x7e, 0x8e, 0x2f, 0x24, 0x8b,
    0x7c, 0x4f, 0x4d, 0x7b, 0x30, 0x3f, 0x0f, 0x8d,
};
// Phone GPS data characteristic (UUID …-1007). Unencrypted write.
// Companion writes a CSV-encoded GPS sample (lat,lon,speed,course,accuracy)
// at approximately 1 Hz while Phone GPS mode is active.
static const uint8_t PHONE_GPS_UUID128[ESP_UUID_LEN_128] = {
    0x07, 0x10, 0x4e, 0x7e, 0x8e, 0x2f, 0x24, 0x8b,
    0x7c, 0x4f, 0x4d, 0x7b, 0x30, 0x3f, 0x0f, 0x8d,
};

static hosted_ble_chunk_cb_t s_chunk_cb;
static hosted_ble_pairing_confirm_cb_t s_pairing_confirm_cb;
static hosted_ble_pairing_request_cb_t s_pairing_request_cb;
static hosted_ble_is_pairing_mode_cb_t s_is_pairing_mode_cb;
static hosted_ble_auth_cmpl_cb_t s_auth_cmpl_cb;
static hosted_ble_phone_gps_cb_t s_phone_gps_cb;
static void *s_chunk_ctx;

static esp_gatt_if_t s_gatts_if = ESP_GATT_IF_NONE;
// Atomic so the BTM task and the publish path don't race on the
// 32-bit slot. RV32 word-aligned 32-bit reads/writes happen to be
// atomic on this hardware so the previous `volatile` was correct in
// practice, but `_Atomic` is what the C standard requires for shared
// state read by both the BT host task and the runtime task. Lock-free
// on every esp32p4 toolchain we ship.
static _Atomic int32_t s_conn_id = -1;
_Static_assert(ATOMIC_INT_LOCK_FREE == 2,
               "this build expects atomic_int to be lock-free");
static uint16_t s_service_handle;
static uint16_t s_chunk_char_handle;
static uint16_t s_event_char_handle;
static uint16_t s_pairing_char_handle;
static uint16_t s_pairing_request_char_handle;
static uint16_t s_phone_gps_char_handle;
// Tracks which of the two advertising payloads (main + scan response)
// have been accepted by the controller. We only call
// `esp_ble_gap_start_advertising` once both have landed.
static bool s_adv_data_set;
static bool s_scan_rsp_set;

// Main advertising packet: flags + 128-bit service UUID. The name and
// appearance go in the scan response below — splitting them keeps both
// payloads under the 31-byte legacy-adv budget. We deliberately leave
// `min_interval` / `max_interval` at 0 so Bluedroid does NOT add a
// "Slave Connection Interval Range" AD field; with the 18-byte 128-bit
// UUID + 3-byte flags, that range field's 6 bytes plus IDF's internal
// overhead pushed the main adv over 31 bytes and the 128-bit UUID was
// being dropped, which prevents iOS / Android scanners from finding us
// via service-UUID filtering.
static esp_ble_adv_data_t s_adv_data = {
    .set_scan_rsp = false,
    .include_name = false,
    .include_txpower = false,
    .min_interval = 0,
    .max_interval = 0,
    .appearance = 0,
    .manufacturer_len = 0,
    .p_manufacturer_data = NULL,
    .service_data_len = 0,
    .p_service_data = NULL,
    .service_uuid_len = ESP_UUID_LEN_128,
    .p_service_uuid = (uint8_t *)SERVICE_UUID128,
    .flag = (ESP_BLE_ADV_FLAG_GEN_DISC | ESP_BLE_ADV_FLAG_BREDR_NOT_SPT),
};

// Scan response: name + appearance, returned when the central does an
// active scan. Pairs with the main adv above.
static esp_ble_adv_data_t s_scan_rsp_data = {
    .set_scan_rsp = true,
    .include_name = true,
    .include_txpower = false,
    .appearance = 0x0480, // generic cycling
    .manufacturer_len = 0,
    .p_manufacturer_data = NULL,
    .service_data_len = 0,
    .p_service_data = NULL,
    .service_uuid_len = 0,
    .p_service_uuid = NULL,
    .flag = 0,
};

static esp_ble_adv_params_t s_adv_params = {
    .adv_int_min = 0x20,
    .adv_int_max = 0x40,
    .adv_type = ADV_TYPE_IND,
    .own_addr_type = BLE_ADDR_TYPE_PUBLIC,
    .channel_map = ADV_CHNL_ALL,
    .adv_filter_policy = ADV_FILTER_ALLOW_SCAN_ANY_CON_ANY,
};

static void start_advertising(void)
{
    esp_err_t err = esp_ble_gap_start_advertising(&s_adv_params);
    if (err != ESP_OK) {
        ESP_LOGW(TAG, "esp_ble_gap_start_advertising -> %s", esp_err_to_name(err));
    }
}

static bool uuid_matches(const esp_bt_uuid_t *uuid, const uint8_t expected[ESP_UUID_LEN_128])
{
    return uuid->len == ESP_UUID_LEN_128 &&
           memcmp(uuid->uuid.uuid128, expected, ESP_UUID_LEN_128) == 0;
}

static void gap_event_handler(esp_gap_ble_cb_event_t event, esp_ble_gap_cb_param_t *param)
{
    switch (event) {
    case ESP_GAP_BLE_ADV_DATA_SET_COMPLETE_EVT:
        s_adv_data_set = true;
        if (s_scan_rsp_set) {
            start_advertising();
        }
        break;
    case ESP_GAP_BLE_ADV_DATA_RAW_SET_COMPLETE_EVT:
        s_adv_data_set = true;
        if (s_scan_rsp_set) {
            start_advertising();
        }
        break;
    case ESP_GAP_BLE_SCAN_RSP_DATA_SET_COMPLETE_EVT:
        s_scan_rsp_set = true;
        if (s_adv_data_set) {
            start_advertising();
        }
        break;
    case ESP_GAP_BLE_ADV_START_COMPLETE_EVT:
        if (param->adv_start_cmpl.status != ESP_BT_STATUS_SUCCESS) {
            ESP_LOGW(TAG, "advertising start failed: %d", param->adv_start_cmpl.status);
        } else {
            ESP_LOGI(TAG, "advertising as \"%s\"", HOSTED_BLE_DEVICE_NAME);
        }
        break;
    case ESP_GAP_BLE_ADV_STOP_COMPLETE_EVT:
        ESP_LOGI(TAG, "advertising stopped");
        break;
    // -------- SMP / pairing observability --------
    // These cases mirror Espressif's `ble_security` example. The Just
    // Works (NoIO) flow shouldn't fire the passkey paths; logging
    // them anyway means a future surprise is visible immediately
    // rather than dropping silently into the `default` case.
    case ESP_GAP_BLE_AUTH_CMPL_EVT: {
        const esp_ble_auth_cmpl_t *cmpl = &param->ble_security.auth_cmpl;
        if (cmpl->success) {
            ESP_LOGI(TAG,
                     "SMP auth_cmpl SUCCESS — bd=%02x:%02x:%02x:%02x:%02x:%02x "
                     "addr_type=%d auth_mode=0x%02x",
                     cmpl->bd_addr[0], cmpl->bd_addr[1], cmpl->bd_addr[2],
                     cmpl->bd_addr[3], cmpl->bd_addr[4], cmpl->bd_addr[5],
                     cmpl->addr_type, (unsigned)cmpl->auth_mode);
        } else {
            ESP_LOGW(TAG,
                     "SMP auth_cmpl FAILURE — bd=%02x:%02x:%02x:%02x:%02x:%02x "
                     "fail_reason=0x%02x (%u) auth_mode=0x%02x",
                     cmpl->bd_addr[0], cmpl->bd_addr[1], cmpl->bd_addr[2],
                     cmpl->bd_addr[3], cmpl->bd_addr[4], cmpl->bd_addr[5],
                     (unsigned)cmpl->fail_reason, (unsigned)cmpl->fail_reason,
                     (unsigned)cmpl->auth_mode);
        }
        if (s_auth_cmpl_cb != NULL) {
            s_auth_cmpl_cb(cmpl->success,
                           (uint8_t)cmpl->fail_reason,
                           cmpl->bd_addr,
                           (uint8_t)cmpl->addr_type,
                           s_chunk_ctx);
        }
        break;
    }
    case ESP_GAP_BLE_KEY_EVT:
        ESP_LOGI(TAG, "SMP key_evt — key_type=0x%02x",
                 (unsigned)param->ble_security.ble_key.key_type);
        break;
    case ESP_GAP_BLE_SEC_REQ_EVT:
        // Peer asked us to start security. Always accept — the OOB
        // secret is what gates the bond at the application layer; SMP
        // just provides link-layer encryption on top.
        ESP_LOGI(TAG, "SMP sec_req — replying yes");
        esp_ble_gap_security_rsp(param->ble_security.ble_req.bd_addr, true);
        break;
    case ESP_GAP_BLE_NC_REQ_EVT:
        // Numeric comparison for SC. With NoIO/NoIO this shouldn't
        // fire (Just Works skips NC), but if it does, accept silently
        // — we can't display a code anyway.
        ESP_LOGI(TAG, "SMP nc_req (passkey 0x%08x) — replying yes",
                 (unsigned)param->ble_security.key_notif.passkey);
        esp_ble_confirm_reply(param->ble_security.key_notif.bd_addr, true);
        break;
    case ESP_GAP_BLE_PASSKEY_REQ_EVT:
        ESP_LOGW(TAG,
                 "SMP passkey_req fired (NoIO shouldn't see this) — replying 0");
        esp_ble_passkey_reply(param->ble_security.ble_req.bd_addr, true, 0);
        break;
    case ESP_GAP_BLE_PASSKEY_NOTIF_EVT:
        ESP_LOGW(TAG,
                 "SMP passkey_notif fired (NoIO shouldn't see this): 0x%08x",
                 (unsigned)param->ble_security.key_notif.passkey);
        break;
    case ESP_GAP_BLE_LOCAL_IR_EVT:
        ESP_LOGI(TAG, "SMP local IR event (random IR generated)");
        break;
    case ESP_GAP_BLE_LOCAL_ER_EVT:
        ESP_LOGI(TAG, "SMP local ER event (random ER generated)");
        break;
    default:
        break;
    }
}

static void gatts_event_handler(esp_gatts_cb_event_t event,
                                esp_gatt_if_t gatts_if,
                                esp_ble_gatts_cb_param_t *param)
{
    switch (event) {
    case ESP_GATTS_REG_EVT: {
        if (param->reg.status != ESP_GATT_OK) {
            ESP_LOGE(TAG, "gatts register failed: %d", param->reg.status);
            return;
        }
        s_gatts_if = gatts_if;

        esp_err_t err = esp_ble_gap_set_device_name(HOSTED_BLE_DEVICE_NAME);
        if (err != ESP_OK) {
            ESP_LOGW(TAG, "set_device_name -> %s", esp_err_to_name(err));
        }
        err = esp_ble_gap_config_adv_data(&s_adv_data);
        if (err != ESP_OK) {
            ESP_LOGW(TAG, "config_adv_data -> %s", esp_err_to_name(err));
        }
        err = esp_ble_gap_config_adv_data(&s_scan_rsp_data);
        if (err != ESP_OK) {
            ESP_LOGW(TAG, "config_scan_rsp_data -> %s", esp_err_to_name(err));
        }

        esp_gatt_srvc_id_t srvc;
        memset(&srvc, 0, sizeof(srvc));
        srvc.is_primary = true;
        srvc.id.inst_id = 0;
        srvc.id.uuid.len = ESP_UUID_LEN_128;
        memcpy(srvc.id.uuid.uuid.uuid128, SERVICE_UUID128, ESP_UUID_LEN_128);
        esp_ble_gatts_create_service(gatts_if, &srvc, HOSTED_BLE_SERVICE_NUM_HANDLE);
        break;
    }
    case ESP_GATTS_CREATE_EVT: {
        if (param->create.status != ESP_GATT_OK) {
            ESP_LOGE(TAG, "service create failed: %d", param->create.status);
            return;
        }
        s_service_handle = param->create.service_handle;

        // The three data characteristics are encrypted: the first
        // touch from a companion triggers SMP Just Works pairing
        // transparently. The OOB secret + single-bond policy still
        // gate the application-layer bond (see App::ingest_pairing_confirm),
        // but link-layer encryption keeps route geometry off the air.
        // pairing_request stays plain so it works *before* SMP runs.
        esp_bt_uuid_t chunk_uuid;
        memset(&chunk_uuid, 0, sizeof(chunk_uuid));
        chunk_uuid.len = ESP_UUID_LEN_128;
        memcpy(chunk_uuid.uuid.uuid128, CHUNK_UUID128, ESP_UUID_LEN_128);
        esp_ble_gatts_add_char(s_service_handle, &chunk_uuid,
                               ESP_GATT_PERM_WRITE_ENCRYPTED,
                               ESP_GATT_CHAR_PROP_BIT_WRITE | ESP_GATT_CHAR_PROP_BIT_WRITE_NR,
                               NULL, NULL);

        esp_bt_uuid_t event_uuid;
        memset(&event_uuid, 0, sizeof(event_uuid));
        event_uuid.len = ESP_UUID_LEN_128;
        memcpy(event_uuid.uuid.uuid128, EVENT_UUID128, ESP_UUID_LEN_128);
        esp_ble_gatts_add_char(s_service_handle, &event_uuid,
                               ESP_GATT_PERM_READ_ENCRYPTED,
                               ESP_GATT_CHAR_PROP_BIT_NOTIFY,
                               NULL, NULL);

        // Pairing-confirm: encrypted write. The first secret-write
        // from the companion triggers SMP Just Works pairing on the
        // host side; once the link is encrypted the C handler reads
        // the 32-byte OOB secret and the App matches it against the
        // current QR's secret. Belt-and-braces: SMP gives link
        // encryption + bond persistence, the OOB secret gates the
        // application-layer bond.
        esp_bt_uuid_t pairing_uuid;
        memset(&pairing_uuid, 0, sizeof(pairing_uuid));
        pairing_uuid.len = ESP_UUID_LEN_128;
        memcpy(pairing_uuid.uuid.uuid128, PAIRING_UUID128, ESP_UUID_LEN_128);
        esp_ble_gatts_add_char(s_service_handle, &pairing_uuid,
                               ESP_GATT_PERM_WRITE_ENCRYPTED,
                               ESP_GATT_CHAR_PROP_BIT_WRITE,
                               NULL, NULL);

        // Pairing-request: UNENCRYPTED write. Companion uses this to
        // ask the device to show its QR before any bonding happens.
        // The payload value is ignored — the existence of the write
        // is the signal.
        esp_bt_uuid_t pairing_request_uuid;
        memset(&pairing_request_uuid, 0, sizeof(pairing_request_uuid));
        pairing_request_uuid.len = ESP_UUID_LEN_128;
        memcpy(pairing_request_uuid.uuid.uuid128, PAIRING_REQUEST_UUID128,
               ESP_UUID_LEN_128);
        esp_ble_gatts_add_char(s_service_handle, &pairing_request_uuid,
                               ESP_GATT_PERM_WRITE,
                               ESP_GATT_CHAR_PROP_BIT_WRITE,
                               NULL, NULL);

        // Phone GPS data: unencrypted write. Companion writes CSV-encoded
        // GPS samples at ~1 Hz. The C trampoline forwards each write to
        // the Rust side where it is parsed and made available to the
        // platform layer's GPS provider selection.
        esp_bt_uuid_t phone_gps_uuid;
        memset(&phone_gps_uuid, 0, sizeof(phone_gps_uuid));
        phone_gps_uuid.len = ESP_UUID_LEN_128;
        memcpy(phone_gps_uuid.uuid.uuid128, PHONE_GPS_UUID128, ESP_UUID_LEN_128);
        esp_ble_gatts_add_char(s_service_handle, &phone_gps_uuid,
                               ESP_GATT_PERM_WRITE,
                               ESP_GATT_CHAR_PROP_BIT_WRITE,
                               NULL, NULL);
        break;
    }
    case ESP_GATTS_ADD_CHAR_EVT: {
        if (param->add_char.status != ESP_GATT_OK) {
            ESP_LOGE(TAG, "add_char failed: %d", param->add_char.status);
            return;
        }
        if (uuid_matches(&param->add_char.char_uuid, CHUNK_UUID128)) {
            s_chunk_char_handle = param->add_char.attr_handle;
        } else if (uuid_matches(&param->add_char.char_uuid, EVENT_UUID128)) {
            s_event_char_handle = param->add_char.attr_handle;

            // Companions need a Client Characteristic Configuration descriptor
            // on the notify characteristic so they can subscribe.
            esp_bt_uuid_t cccd_uuid;
            memset(&cccd_uuid, 0, sizeof(cccd_uuid));
            cccd_uuid.len = ESP_UUID_LEN_16;
            cccd_uuid.uuid.uuid16 = ESP_GATT_UUID_CHAR_CLIENT_CONFIG;
            esp_ble_gatts_add_char_descr(s_service_handle, &cccd_uuid,
                                         ESP_GATT_PERM_READ | ESP_GATT_PERM_WRITE,
                                         NULL, NULL);
        } else if (uuid_matches(&param->add_char.char_uuid, PAIRING_UUID128)) {
            s_pairing_char_handle = param->add_char.attr_handle;
        } else if (uuid_matches(&param->add_char.char_uuid,
                                PAIRING_REQUEST_UUID128)) {
            s_pairing_request_char_handle = param->add_char.attr_handle;
        } else if (uuid_matches(&param->add_char.char_uuid, PHONE_GPS_UUID128)) {
            s_phone_gps_char_handle = param->add_char.attr_handle;
        }

        if (s_chunk_char_handle != 0 && s_event_char_handle != 0 &&
            s_pairing_char_handle != 0 && s_pairing_request_char_handle != 0 &&
            s_phone_gps_char_handle != 0) {
            esp_ble_gatts_start_service(s_service_handle);
        }
        break;
    }
    case ESP_GATTS_ADD_CHAR_DESCR_EVT:
        if (param->add_char_descr.status != ESP_GATT_OK) {
            ESP_LOGW(TAG, "add_char_descr failed: %d", param->add_char_descr.status);
        }
        break;
    case ESP_GATTS_START_EVT:
        ESP_LOGI(TAG, "route-sync GATT service started (handle=%u)", param->start.service_handle);
        break;
    case ESP_GATTS_CONNECT_EVT: {
        // Single-bond enforcement at the connection layer: if a peer is
        // already connected, reject the new one. Mirrors
        // `BleConnectionState::on_connect` in `firmware/src/hosted_ble_state.rs`,
        // which is the host-testable rule reference.
        int32_t expected = -1;
        if (atomic_compare_exchange_strong(&s_conn_id, &expected,
                                           (int32_t)param->connect.conn_id)) {
            esp_ble_gap_stop_advertising();
            ESP_LOGI(TAG, "companion connected: conn_id=%u", param->connect.conn_id);
        } else {
            ESP_LOGW(TAG,
                     "rejecting duplicate connection conn_id=%u while "
                     "conn_id=%" PRId32 " is still active",
                     param->connect.conn_id, expected);
            esp_ble_gap_disconnect(param->connect.remote_bda);
        }
        break;
    }
    case ESP_GATTS_DISCONNECT_EVT: {
        // Only clear the slot if the disconnect is for the active peer;
        // stale events from a rejected duplicate connection must not
        // knock the active session offline.
        int32_t expected = (int32_t)param->disconnect.conn_id;
        atomic_compare_exchange_strong(&s_conn_id, &expected, -1);
        ESP_LOGI(TAG, "companion disconnected: conn_id=%u reason=0x%02x",
                 param->disconnect.conn_id, param->disconnect.reason);
        start_advertising();
        break;
    }
    case ESP_GATTS_WRITE_EVT: {
        esp_gatt_status_t write_status = ESP_GATT_OK;
        if (param->write.handle == s_chunk_char_handle && s_chunk_cb != NULL &&
            param->write.value != NULL && param->write.len > 0) {
            s_chunk_cb(param->write.value, param->write.len, s_chunk_ctx);
        } else if (param->write.handle == s_pairing_request_char_handle) {
            // Companion asked the device to enter pairing mode. The
            // value is ignored — the write itself is the signal. No
            // SMP pairing required: the write reaches us over an
            // unencrypted link so the user can scan the QR before any
            // bonding has happened.
            ESP_LOGI(TAG, "pairing_request received; flagging Rust-side queue");
            if (s_pairing_request_cb != NULL) {
                s_pairing_request_cb(s_chunk_ctx);
            }
        } else if (param->write.handle == s_phone_gps_char_handle &&
                   s_phone_gps_cb != NULL &&
                   param->write.value != NULL && param->write.len > 0) {
            s_phone_gps_cb(param->write.value, param->write.len, s_chunk_ctx);
        } else if (param->write.handle == s_pairing_char_handle) {
            // Single-bond policy: refuse pairing-confirm writes once the
            // device is bonded. The companion-side `forgetPairedDevice`
            // flow is the only way back to pairing mode.
            bool accept = (s_is_pairing_mode_cb != NULL) &&
                          s_is_pairing_mode_cb(s_chunk_ctx);
            if (!accept) {
                ESP_LOGW(TAG,
                         "rejecting pairing_confirm write while not in pairing "
                         "mode (single-bond policy)");
                write_status = ESP_GATT_WRITE_NOT_PERMIT;
            } else if (s_pairing_confirm_cb != NULL && param->write.value != NULL &&
                       param->write.len == HOSTED_BLE_PAIRING_SECRET_LEN) {
                s_pairing_confirm_cb(param->write.value, param->write.len,
                                     s_chunk_ctx);
            } else {
                ESP_LOGW(TAG,
                         "dropping pairing_confirm write of unexpected length %u "
                         "(expected %u)",
                         (unsigned)param->write.len,
                         (unsigned)HOSTED_BLE_PAIRING_SECRET_LEN);
                write_status = ESP_GATT_INVALID_ATTR_LEN;
            }
        }
        if (param->write.need_rsp) {
            esp_ble_gatts_send_response(gatts_if, param->write.conn_id,
                                        param->write.trans_id, write_status, NULL);
        }
        break;
    }
    case ESP_GATTS_MTU_EVT:
        ESP_LOGI(TAG, "negotiated MTU=%u", param->mtu.mtu);
        break;
    default:
        break;
    }
}

esp_err_t hosted_ble_route_sync_start(const hosted_ble_route_sync_callbacks_t *cb)
{
    if (cb == NULL) {
        return ESP_ERR_INVALID_ARG;
    }

    esp_err_t err = hosted_ble_init();
    if (err != ESP_OK) {
        return err;
    }

    s_chunk_cb = cb->on_chunk;
    s_pairing_confirm_cb = cb->on_pairing_confirm;
    s_pairing_request_cb = cb->on_pairing_request;
    s_is_pairing_mode_cb = cb->is_pairing_mode;
    s_auth_cmpl_cb = cb->on_auth_cmpl;
    s_phone_gps_cb = cb->on_phone_gps;
    s_chunk_ctx = cb->ctx;

    err = esp_ble_gap_register_callback(gap_event_handler);
    if (err != ESP_OK) {
        ESP_LOGE(TAG, "gap_register_callback: %s", esp_err_to_name(err));
        return err;
    }
    err = esp_ble_gatts_register_callback(gatts_event_handler);
    if (err != ESP_OK) {
        ESP_LOGE(TAG, "gatts_register_callback: %s", esp_err_to_name(err));
        return err;
    }
    err = esp_ble_gatts_app_register(HOSTED_BLE_APP_ID);
    if (err != ESP_OK) {
        ESP_LOGE(TAG, "gatts_app_register: %s", esp_err_to_name(err));
        return err;
    }

    // Negotiate up to a 512-byte MTU so the largest BLE packets in the
    // route-sync wire format fit in a single ATT MTU when the companion
    // supports it. Companions that cap MTU lower will negotiate down.
    err = esp_ble_gatt_set_local_mtu(HOSTED_BLE_MAX_PACKET_LEN);
    if (err != ESP_OK) {
        ESP_LOGW(TAG, "set_local_mtu(%d) -> %s", HOSTED_BLE_MAX_PACKET_LEN, esp_err_to_name(err));
    }
    return ESP_OK;
}

esp_err_t hosted_ble_route_sync_set_adv_filter(bool whitelist_only,
                                               const uint8_t peer_addr[6],
                                               uint8_t addr_type)
{
    if (whitelist_only) {
        if (peer_addr == NULL) {
            return ESP_ERR_INVALID_ARG;
        }
        // Loading the whitelist before flipping the filter policy
        // prevents a race where the controller would briefly reject all
        // connection attempts (including from the bonded phone) because
        // the whitelist was empty.
        esp_bd_addr_t bd;
        memcpy(bd, peer_addr, sizeof(bd));
        esp_err_t err = esp_ble_gap_update_whitelist(true, bd, addr_type);
        if (err != ESP_OK) {
            ESP_LOGW(TAG, "esp_ble_gap_update_whitelist -> %s",
                     esp_err_to_name(err));
            return err;
        }
        s_adv_params.adv_filter_policy = ADV_FILTER_ALLOW_SCAN_WLST_CON_WLST;
    } else {
        s_adv_params.adv_filter_policy = ADV_FILTER_ALLOW_SCAN_ANY_CON_ANY;
    }
    // The next ADV_DATA_SET_COMPLETE → start_advertising() will pick up
    // the new filter policy. If we're already advertising, stop+start
    // so the change takes effect immediately.
    esp_ble_gap_stop_advertising();
    start_advertising();
    return ESP_OK;
}

esp_err_t hosted_ble_route_sync_notify(const uint8_t *data, size_t len)
{
    if (data == NULL || len == 0) {
        return ESP_ERR_INVALID_ARG;
    }
    int32_t conn_id = atomic_load(&s_conn_id);
    if (conn_id < 0 || s_event_char_handle == 0 || s_gatts_if == ESP_GATT_IF_NONE) {
        return ESP_ERR_INVALID_STATE;
    }
    return esp_ble_gatts_send_indicate(s_gatts_if, (uint16_t)conn_id,
                                       s_event_char_handle, (uint16_t)len,
                                       (uint8_t *)data, false);
}
