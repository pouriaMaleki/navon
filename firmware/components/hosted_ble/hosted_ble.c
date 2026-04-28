#include "hosted_ble.h"

#include "esp_bluedroid_hci.h"
#include "esp_bt_defs.h"
#include "esp_bt_main.h"
#include "esp_gap_ble_api.h"
#include "esp_log.h"

#include "esp_hosted.h"
#include "esp_hosted_bluedroid.h"
#include "esp_hosted_misc.h"

static const char *TAG = "hosted_ble";

static bool s_initialized = false;

esp_err_t hosted_ble_init(void)
{
    esp_err_t err;

    if (s_initialized) {
        return ESP_OK;
    }

    // Bring up the SDIO transport to the ESP32-C6. esp_hosted negotiates
    // the slave's capabilities (BT + Wi-Fi) and starts the rx/tx threads
    // on the host side. Idempotent within esp_hosted itself, but we still
    // gate with `s_initialized` so repeated callers don't spin up extra
    // tasks.
    int rc = esp_hosted_connect_to_slave();
    if (rc != 0) {
        ESP_LOGE(TAG, "esp_hosted_connect_to_slave -> %d", rc);
        return ESP_FAIL;
    }

    // The "controller" on a hosted host is a thin RPC shim that asks the
    // C6 to set up its BT subsystem. Older slave firmware (e.g. the
    // v0.0.6 image Espressif and Waveshare pre-flash on dev kits) doesn't
    // implement the `Req_FeatureControl` RPC this maps to, but those
    // slaves still boot BT automatically. Espressif's own
    // `host_bluedroid_host_only` reference treats both calls as advisory
    // (logs a warning and continues) — we mirror that so the HCI bridge
    // below comes up cleanly against any slave version.
    err = esp_hosted_bt_controller_init();
    if (err != ESP_OK) {
        ESP_LOGW(TAG, "esp_hosted_bt_controller_init returned %s — slave BT is "
                      "expected to be self-starting; continuing with HCI bridge",
                 esp_err_to_name(err));
    }
    err = esp_hosted_bt_controller_enable();
    if (err != ESP_OK) {
        ESP_LOGW(TAG, "esp_hosted_bt_controller_enable returned %s — continuing",
                 esp_err_to_name(err));
    }

    // Open the hosted HCI transport (SDIO channel to C6) and register its
    // send/recv callbacks with Bluedroid. This must happen *before*
    // esp_bluedroid_init() so the host stack can find a working HCI on its
    // first command. Pattern is from
    // managed_components/espressif__esp_hosted/examples/host_bluedroid_host_only.
    hosted_hci_bluedroid_open();

    esp_bluedroid_hci_driver_operations_t ops = {
        .send = hosted_hci_bluedroid_send,
        .check_send_available = hosted_hci_bluedroid_check_send_available,
        .register_host_callback = hosted_hci_bluedroid_register_host_callback,
    };
    err = esp_bluedroid_attach_hci_driver(&ops);
    if (err != ESP_OK) {
        ESP_LOGE(TAG, "esp_bluedroid_attach_hci_driver failed: %s",
                 esp_err_to_name(err));
        return err;
    }

    err = esp_bluedroid_init();
    if (err != ESP_OK) {
        ESP_LOGE(TAG, "esp_bluedroid_init failed: %s", esp_err_to_name(err));
        return err;
    }

    err = esp_bluedroid_enable();
    if (err != ESP_OK) {
        ESP_LOGE(TAG, "esp_bluedroid_enable failed: %s", esp_err_to_name(err));
        return err;
    }

    // Configure SMP for Legacy Pairing + bonding ("Just Works" at the
    // link layer; OOB-secret confirmation on top via the
    // pairing_confirm characteristic provides MITM resistance). The
    // resulting bond is persisted to NVS by Bluedroid automatically
    // because we set CONFIG_BT_BLE_SMP_BOND_NVS_FLASH=y.
    //
    // We deliberately use Legacy (`ESP_LE_AUTH_BOND`) rather than
    // LE Secure Connections (`ESP_LE_AUTH_REQ_SC_BOND`). On the P4
    // hosted-HCI path through the C6 controller, SC's ECDH key
    // exchange has been failing with `auth_cmpl rsn 99` — Legacy
    // Just Works only needs key transport, which is more tolerant
    // of controller-firmware version skew. Once SMP is verified to
    // work end-to-end against the matching slave (cargo xtask
    // build-c6-slave), we can promote back to SC.
    esp_ble_auth_req_t auth_req = ESP_LE_AUTH_BOND;
    esp_ble_io_cap_t iocap = ESP_IO_CAP_NONE;
    uint8_t key_size = 16;
    // Tell Bluedroid we don't have BLE-layer OOB data to exchange —
    // our OOB is application-layer (the QR secret travels over a
    // cleartext characteristic). Without this Bluedroid may signal
    // OOB-required to the peer and SMP fails before key transport.
    uint8_t oob_support = 0;
    // Initiator + responder key distribution: encryption (LTK), id
    // (IRK + identity address). Csrk is unused for our GATT traffic.
    uint8_t init_key = ESP_BLE_ENC_KEY_MASK | ESP_BLE_ID_KEY_MASK;
    uint8_t rsp_key = ESP_BLE_ENC_KEY_MASK | ESP_BLE_ID_KEY_MASK;

    esp_err_t sec_err = esp_ble_gap_set_security_param(ESP_BLE_SM_AUTHEN_REQ_MODE,
                                                      &auth_req, sizeof(auth_req));
    if (sec_err != ESP_OK) {
        ESP_LOGW(TAG, "esp_ble_gap_set_security_param(AUTHEN_REQ_MODE) -> %s",
                 esp_err_to_name(sec_err));
    }
    sec_err = esp_ble_gap_set_security_param(ESP_BLE_SM_IOCAP_MODE,
                                             &iocap, sizeof(iocap));
    if (sec_err != ESP_OK) {
        ESP_LOGW(TAG, "esp_ble_gap_set_security_param(IOCAP_MODE) -> %s",
                 esp_err_to_name(sec_err));
    }
    sec_err = esp_ble_gap_set_security_param(ESP_BLE_SM_MAX_KEY_SIZE,
                                             &key_size, sizeof(key_size));
    if (sec_err != ESP_OK) {
        ESP_LOGW(TAG, "esp_ble_gap_set_security_param(MAX_KEY_SIZE) -> %s",
                 esp_err_to_name(sec_err));
    }
    sec_err = esp_ble_gap_set_security_param(ESP_BLE_SM_OOB_SUPPORT,
                                             &oob_support, sizeof(oob_support));
    if (sec_err != ESP_OK) {
        ESP_LOGW(TAG, "esp_ble_gap_set_security_param(OOB_SUPPORT) -> %s",
                 esp_err_to_name(sec_err));
    }
    sec_err = esp_ble_gap_set_security_param(ESP_BLE_SM_SET_INIT_KEY,
                                             &init_key, sizeof(init_key));
    if (sec_err != ESP_OK) {
        ESP_LOGW(TAG, "esp_ble_gap_set_security_param(SET_INIT_KEY) -> %s",
                 esp_err_to_name(sec_err));
    }
    sec_err = esp_ble_gap_set_security_param(ESP_BLE_SM_SET_RSP_KEY,
                                             &rsp_key, sizeof(rsp_key));
    if (sec_err != ESP_OK) {
        ESP_LOGW(TAG, "esp_ble_gap_set_security_param(SET_RSP_KEY) -> %s",
                 esp_err_to_name(sec_err));
    }

    s_initialized = true;
    ESP_LOGI(TAG, "BLE host stack online via ESP32-C6 over hosted SDIO");
    return ESP_OK;
}
