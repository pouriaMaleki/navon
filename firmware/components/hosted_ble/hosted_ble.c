#include "hosted_ble.h"

#include "esp_bluedroid_hci.h"
#include "esp_bt_main.h"
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

    s_initialized = true;
    ESP_LOGI(TAG, "BLE host stack online via ESP32-C6 over hosted SDIO");
    return ESP_OK;
}
