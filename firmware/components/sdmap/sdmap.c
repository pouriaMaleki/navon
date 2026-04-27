// Read-only SD card loader for /sdcard/map.svm. See sdmap.h.

#include "sdmap.h"

#include <errno.h>
#include <stdio.h>
#include <string.h>

#include "driver/sdmmc_host.h"
#include "esp_err.h"
#include "esp_heap_caps.h"
#include "esp_log.h"
#include "esp_vfs_fat.h"
#include "sd_pwr_ctrl_by_on_chip_ldo.h"
#include "sdmmc_cmd.h"

static const char *TAG = "sdmap";
static const char MOUNT_POINT[] = "/sdcard";
static const char MAP_FILE_PATH[] = "/sdcard/map.svm";

uint8_t *sdmap_load(size_t *out_len) {
    if (out_len) {
        *out_len = 0;
    }

    // SAFETY-CRITICAL: format_if_mount_failed=false guarantees a non-FAT or
    // empty card is left untouched. We never call f_mkfs or any write API.
    esp_vfs_fat_sdmmc_mount_config_t mount_config = {
        .format_if_mount_failed = false,
        .max_files = 2,
        .allocation_unit_size = 0,
        .disk_status_check_enable = false,
    };

    // Waveshare ESP32-P4-WIFI6-Touch-LCD-3.4C wires the TF socket to SDMMC
    // slot 0 (CLK=43, CMD=44, D0-D3=39-42) and powers the card through the
    // SoC's on-chip LDO channel 4. Without enabling that LDO every CMD0/OCR
    // probe times out (ESP_ERR_TIMEOUT 0x107). The slot index and the
    // pwr_ctrl_handle are the only deltas from the generic ESP32-P4 default.
    sdmmc_host_t host = SDMMC_HOST_DEFAULT();
    host.slot = SDMMC_HOST_SLOT_0;
    host.max_freq_khz = SDMMC_FREQ_HIGHSPEED;

    sd_pwr_ctrl_ldo_config_t ldo_config = {
        .ldo_chan_id = 4,
    };
    sd_pwr_ctrl_handle_t pwr_ctrl_handle = NULL;
    esp_err_t pwr_err = sd_pwr_ctrl_new_on_chip_ldo(&ldo_config, &pwr_ctrl_handle);
    if (pwr_err != ESP_OK) {
        ESP_LOGW(TAG, "sd LDO init failed: %s — falling back to flash partition",
                 esp_err_to_name(pwr_err));
        return NULL;
    }
    host.pwr_ctrl_handle = pwr_ctrl_handle;

    sdmmc_slot_config_t slot_config = {
        .cd = SDMMC_SLOT_NO_CD,
        .wp = SDMMC_SLOT_NO_WP,
        .width = 4,
        .flags = 0,
    };

    sdmmc_card_t *card = NULL;
    esp_err_t err = esp_vfs_fat_sdmmc_mount(MOUNT_POINT, &host, &slot_config,
                                            &mount_config, &card);
    if (err != ESP_OK) {
        ESP_LOGW(TAG,
                 "sd mount failed: %s (0x%x) — falling back to flash partition",
                 esp_err_to_name(err), err);
        sd_pwr_ctrl_del_on_chip_ldo(pwr_ctrl_handle);
        return NULL;
    }

    ESP_LOGI(TAG, "sd card mounted: %lluMB",
             ((uint64_t)card->csd.capacity) * card->csd.sector_size /
                 (1024ULL * 1024ULL));

    uint8_t *buf = NULL;
    size_t total = 0;

    FILE *f = fopen(MAP_FILE_PATH, "rb");
    if (!f) {
        ESP_LOGW(TAG, "fopen %s failed: %s — falling back to flash partition",
                 MAP_FILE_PATH, strerror(errno));
        goto cleanup;
    }

    if (fseek(f, 0, SEEK_END) != 0) {
        ESP_LOGE(TAG, "fseek end failed: %s", strerror(errno));
        goto cleanup;
    }
    long size = ftell(f);
    if (size <= 0) {
        ESP_LOGE(TAG, "invalid file size %ld", size);
        goto cleanup;
    }
    rewind(f);

    ESP_LOGI(TAG, "loading map.svm: %.2f MB", (double)size / 1048576.0);

    buf = (uint8_t *)heap_caps_malloc((size_t)size, MALLOC_CAP_SPIRAM | MALLOC_CAP_8BIT);
    if (!buf) {
        ESP_LOGE(TAG, "PSRAM alloc failed for %ld bytes — file too large", size);
        goto cleanup;
    }

    while (total < (size_t)size) {
        size_t got = fread(buf + total, 1, (size_t)size - total, f);
        if (got == 0) {
            if (feof(f)) {
                break;
            }
            ESP_LOGE(TAG, "fread failed at offset %zu: %s", total, strerror(errno));
            heap_caps_free(buf);
            buf = NULL;
            total = 0;
            goto cleanup;
        }
        total += got;
    }

cleanup:
    if (f) {
        fclose(f);
    }
    esp_err_t unmount_err = esp_vfs_fat_sdcard_unmount(MOUNT_POINT, card);
    if (unmount_err != ESP_OK) {
        ESP_LOGW(TAG, "unmount returned %s", esp_err_to_name(unmount_err));
    }
    sd_pwr_ctrl_del_on_chip_ldo(pwr_ctrl_handle);

    if (buf && out_len) {
        *out_len = total;
        ESP_LOGI(TAG, "sd map loaded: %zu bytes", total);
    }
    return buf;
}

void sdmap_free(uint8_t *buf) {
    if (buf) {
        heap_caps_free(buf);
    }
}
