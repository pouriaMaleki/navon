//! One-shot I²C bus scan for diagnostic use during device bring-up.
//!
//! Uses ESP-IDF's **legacy** I²C driver (`driver/i2c.h`) because
//! `esp_idf_hal` links the legacy implementation and ESP-IDF aborts with
//! `CONFLICT! driver_ng is not allowed to be used with this old driver`
//! the moment a second caller touches the new-generation
//! `driver/i2c_master.h` API. Once firmware migrates to the new driver
//! top-to-bottom we can drop this in favor of `i2c_master_probe`.

#![cfg(target_os = "espidf")]

use esp_idf_svc::sys::{
    self, esp_err_t, i2c_cmd_handle_t, i2c_cmd_link_create, i2c_cmd_link_delete, i2c_config_t,
    i2c_config_t__bindgen_ty_1, i2c_config_t__bindgen_ty_1__bindgen_ty_1, i2c_driver_install,
    i2c_master_cmd_begin, i2c_master_start, i2c_master_stop, i2c_master_write_byte,
    i2c_mode_t_I2C_MODE_MASTER, i2c_param_config, i2c_port_t_I2C_NUM_0,
};

use crate::esp_idf::EspIdfError;

/// Default I²C bus pins on Waveshare ESP32-P4 touch boards. The 3.4C kit
/// routes both the touch controller and the CH422G expander onto this
/// bus; the TouchControllerConfig already uses these.
pub const DEFAULT_SDA_GPIO: i32 = 7;
pub const DEFAULT_SCL_GPIO: i32 = 8;

/// One-shot probe of every valid 7-bit address on the I²C bus at
/// `(sda_gpio, scl_gpio)`. Returns addresses that ACK'd within 50 ms.
/// Also logs each responder via `log::info!`.
pub fn scan_bus(sda_gpio: i32, scl_gpio: i32) -> Result<Vec<u8>, EspIdfError> {
    install_legacy_master(sda_gpio, scl_gpio)?;
    let mut found: Vec<u8> = Vec::new();
    for addr in 0x08u8..=0x77u8 {
        if probe_addr(addr) {
            log::info!("i2c: device ACK at 0x{:02x}", addr);
            found.push(addr);
        }
    }
    if found.is_empty() {
        log::warn!(
            "i2c: no devices found on sda=GPIO{} scl=GPIO{} (pull-ups? wrong pins?)",
            sda_gpio,
            scl_gpio,
        );
    } else {
        log::info!("i2c: scan complete — {} device(s) responded", found.len());
    }
    Ok(found)
}

/// Writes a single byte to `addr` on the already-installed bus. Used for
/// bring-up pokes at suspected I/O expanders (e.g. "turn on backlight"
/// via CH422G).
pub fn write_byte(addr: u8, byte: u8) -> Result<(), EspIdfError> {
    unsafe {
        let cmd = i2c_cmd_link_create();
        check(i2c_master_start(cmd), "i2c_master_start")?;
        check(
            i2c_master_write_byte(cmd, (addr << 1) | 0, true),
            "i2c_master_write_byte(addr)",
        )?;
        check(
            i2c_master_write_byte(cmd, byte, true),
            "i2c_master_write_byte(payload)",
        )?;
        check(i2c_master_stop(cmd), "i2c_master_stop")?;
        let result = i2c_master_cmd_begin(i2c_port_t_I2C_NUM_0, cmd, pd_ms_to_ticks(100));
        i2c_cmd_link_delete(cmd);
        check(result, "i2c_master_cmd_begin")?;
    }
    log::info!("i2c: wrote 0x{:02x} → device 0x{:02x}", byte, addr);
    Ok(())
}

/// Install the legacy I²C master driver on `I2C_NUM_0` if it is not
/// already installed. Idempotent — `ESP_ERR_INVALID_STATE` from
/// `i2c_driver_install` (driver already up) is treated as success so the
/// scanner and the touch driver can both call this without coordinating.
pub fn install_legacy_master_if_needed(
    sda_gpio: i32,
    scl_gpio: i32,
) -> Result<(), EspIdfError> {
    install_legacy_master(sda_gpio, scl_gpio)
}

fn install_legacy_master(sda_gpio: i32, scl_gpio: i32) -> Result<(), EspIdfError> {
    let config = i2c_config_t {
        mode: i2c_mode_t_I2C_MODE_MASTER,
        sda_io_num: sda_gpio,
        scl_io_num: scl_gpio,
        sda_pullup_en: true,
        scl_pullup_en: true,
        __bindgen_anon_1: i2c_config_t__bindgen_ty_1 {
            master: i2c_config_t__bindgen_ty_1__bindgen_ty_1 { clk_speed: 100_000 },
        },
        clk_flags: 0,
    };
    let port = i2c_port_t_I2C_NUM_0;
    let status = unsafe { i2c_param_config(port, &config) };
    if status != sys::ESP_OK as esp_err_t {
        return Err(EspIdfError::Io(format!(
            "i2c_param_config(sda={}, scl={}) failed: {}",
            sda_gpio, scl_gpio, status
        )));
    }
    // `i2c_driver_install(port, mode, 0, 0, 0)` — master needs no rx/tx
    // buffer allocations (target only), zero intr_alloc_flags is fine.
    let status = unsafe { i2c_driver_install(port, i2c_mode_t_I2C_MODE_MASTER, 0, 0, 0) };
    // Two "already installed" codes exist across ESP-IDF versions: the
    // classic ESP_ERR_INVALID_STATE, and — on ESP32-P4's IDF 5.4
    // legacy shim (`components/driver/i2c/i2c.c`) — ESP_FAIL (-1),
    // which the already-installed branch returns directly. Fresh-install
    // failures log distinct messages (malloc/semaphore/IRQ errors)
    // before returning, so tolerating -1 here does not mask those.
    if status == sys::ESP_ERR_INVALID_STATE as esp_err_t || status == sys::ESP_FAIL as esp_err_t {
        // Driver already installed by someone else — reuse it.
        return Ok(());
    }
    if status != sys::ESP_OK as esp_err_t {
        return Err(EspIdfError::Io(format!(
            "i2c_driver_install failed: {}",
            status
        )));
    }
    Ok(())
}

fn probe_addr(addr: u8) -> bool {
    unsafe {
        let cmd: i2c_cmd_handle_t = i2c_cmd_link_create();
        if i2c_master_start(cmd) != sys::ESP_OK as esp_err_t {
            i2c_cmd_link_delete(cmd);
            return false;
        }
        // (addr << 1) | 0 == 7-bit address + WRITE bit. If the target ACKs
        // we stop immediately — we only care about presence, not data.
        if i2c_master_write_byte(cmd, (addr << 1) | 0, true) != sys::ESP_OK as esp_err_t {
            i2c_cmd_link_delete(cmd);
            return false;
        }
        if i2c_master_stop(cmd) != sys::ESP_OK as esp_err_t {
            i2c_cmd_link_delete(cmd);
            return false;
        }
        let result = i2c_master_cmd_begin(i2c_port_t_I2C_NUM_0, cmd, pd_ms_to_ticks(50));
        i2c_cmd_link_delete(cmd);
        result == sys::ESP_OK as esp_err_t
    }
}

fn pd_ms_to_ticks(ms: u32) -> u32 {
    // FreeRTOS tick rate on ESP-IDF defaults to 100 Hz (10 ms / tick).
    // portTICK_PERIOD_MS macro expands to (1000 / configTICK_RATE_HZ).
    // 10 ms per tick here; round up so callers never get a zero timeout.
    (ms + 9) / 10
}

fn check(status: esp_err_t, op: &str) -> Result<(), EspIdfError> {
    if status == sys::ESP_OK as esp_err_t {
        Ok(())
    } else {
        Err(EspIdfError::Io(format!("{} failed: {}", op, status)))
    }
}
