// Route-sync GATT server.
//
// Service / characteristic UUIDs mirror `docs/ble-route-sync-contract.md`:
//
//   service    8d0f3f30-7b4d-4f7c-8b24-2f8e7e4e1001
//   chunk_w    8d0f3f30-7b4d-4f7c-8b24-2f8e7e4e1002  (write / write-no-rsp)
//   event_n    8d0f3f30-7b4d-4f7c-8b24-2f8e7e4e1003  (notify)
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
#define HOSTED_BLE_SERVICE_NUM_HANDLE 8
#define HOSTED_BLE_MAX_PACKET_LEN 512

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

static hosted_ble_chunk_cb_t s_chunk_cb;
static void *s_chunk_ctx;

static esp_gatt_if_t s_gatts_if = ESP_GATT_IF_NONE;
static volatile int32_t s_conn_id = -1;
static uint16_t s_service_handle;
static uint16_t s_chunk_char_handle;
static uint16_t s_event_char_handle;
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

        esp_bt_uuid_t chunk_uuid;
        memset(&chunk_uuid, 0, sizeof(chunk_uuid));
        chunk_uuid.len = ESP_UUID_LEN_128;
        memcpy(chunk_uuid.uuid.uuid128, CHUNK_UUID128, ESP_UUID_LEN_128);
        esp_ble_gatts_add_char(s_service_handle, &chunk_uuid,
                               ESP_GATT_PERM_WRITE,
                               ESP_GATT_CHAR_PROP_BIT_WRITE | ESP_GATT_CHAR_PROP_BIT_WRITE_NR,
                               NULL, NULL);

        esp_bt_uuid_t event_uuid;
        memset(&event_uuid, 0, sizeof(event_uuid));
        event_uuid.len = ESP_UUID_LEN_128;
        memcpy(event_uuid.uuid.uuid128, EVENT_UUID128, ESP_UUID_LEN_128);
        esp_ble_gatts_add_char(s_service_handle, &event_uuid,
                               ESP_GATT_PERM_READ,
                               ESP_GATT_CHAR_PROP_BIT_NOTIFY,
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
        }

        if (s_chunk_char_handle != 0 && s_event_char_handle != 0) {
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
    case ESP_GATTS_CONNECT_EVT:
        s_conn_id = param->connect.conn_id;
        esp_ble_gap_stop_advertising();
        ESP_LOGI(TAG, "companion connected: conn_id=%u", param->connect.conn_id);
        break;
    case ESP_GATTS_DISCONNECT_EVT:
        s_conn_id = -1;
        ESP_LOGI(TAG, "companion disconnected: reason=0x%02x", param->disconnect.reason);
        start_advertising();
        break;
    case ESP_GATTS_WRITE_EVT: {
        if (param->write.handle == s_chunk_char_handle && s_chunk_cb != NULL &&
            param->write.value != NULL && param->write.len > 0) {
            s_chunk_cb(param->write.value, param->write.len, s_chunk_ctx);
        }
        if (param->write.need_rsp) {
            esp_ble_gatts_send_response(gatts_if, param->write.conn_id,
                                        param->write.trans_id, ESP_GATT_OK, NULL);
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

esp_err_t hosted_ble_route_sync_notify(const uint8_t *data, size_t len)
{
    if (data == NULL || len == 0) {
        return ESP_ERR_INVALID_ARG;
    }
    if (s_conn_id < 0 || s_event_char_handle == 0 || s_gatts_if == ESP_GATT_IF_NONE) {
        return ESP_ERR_INVALID_STATE;
    }
    return esp_ble_gatts_send_indicate(s_gatts_if, (uint16_t)s_conn_id,
                                       s_event_char_handle, (uint16_t)len,
                                       (uint8_t *)data, false);
}
