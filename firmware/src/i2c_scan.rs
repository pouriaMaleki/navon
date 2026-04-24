//! One-shot I²C bus scan for diagnostic use during device bring-up.
//!
//! The Waveshare ESP32-P4 boards typically route the LCD panel reset,
//! touch controller, and backlight-enable lines through an I²C I/O
//! expander (most commonly a CH422G at 0x20-0x27, sometimes a TCA/PCA
//! 95xx at 0x38-0x3F). Without knowing which addresses exist on the
//! bus we cannot drive those lines from software. This scan probes
//! every valid 7-bit address, logs the responders, and hands the list
//! back to the caller so it can react (enable an expander, print a
//! warning, etc.).

#![cfg(target_os = "espidf")]

use esp_idf_svc::sys::{
    self, esp_err_t, i2c_clock_source_t, i2c_device_config_t, i2c_master_bus_config_t,
    i2c_master_bus_config_t__bindgen_ty_1, i2c_master_bus_config_t__bindgen_ty_2,
    i2c_master_bus_handle_t, i2c_master_dev_handle_t, i2c_master_probe,
    i2c_master_transmit, i2c_new_master_bus, i2c_port_t_I2C_NUM_0,
    soc_periph_i2c_clk_src_t_I2C_CLK_SRC_DEFAULT,
};

use crate::esp_idf::EspIdfError;

/// Default I²C bus pins on Waveshare ESP32-P4 touch boards. The 3.4C kit
/// routes both the touch controller and the CH422G expander onto this
/// bus; the TouchControllerConfig already uses these.
pub const DEFAULT_SDA_GPIO: i32 = 7;
pub const DEFAULT_SCL_GPIO: i32 = 8;

/// One-shot probe of every valid 7-bit address on the I²C bus at
/// `(sda_gpio, scl_gpio)`. Returns addresses that ACK'd within 50 ms.
/// Also logs each responder via `log::info!` so the serial monitor shows
/// them in boot order.
pub fn scan_bus(sda_gpio: i32, scl_gpio: i32) -> Result<Vec<u8>, EspIdfError> {
    let bus = open_master_bus(sda_gpio, scl_gpio)?;
    let mut found: Vec<u8> = Vec::new();
    for addr in 0x08u8..=0x77u8 {
        let status = unsafe { i2c_master_probe(bus, u16::from(addr), 50) };
        if status == sys::ESP_OK as esp_err_t {
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
        log::info!(
            "i2c: scan complete — {} device(s) responded",
            found.len()
        );
    }
    // Leave the bus handle alive by design — the driver holds it and a
    // later caller may reuse it. For a one-shot scan we simply leak it.
    // Production code should keep the handle and pass it to device
    // constructors; this diagnostic helper is intentionally single-shot.
    Ok(found)
}

/// Writes a single byte to `addr` on `bus`. Used for bring-up pokes at
/// suspected I/O expanders (e.g. turning on backlight via CH422G).
pub fn write_byte(
    sda_gpio: i32,
    scl_gpio: i32,
    addr: u8,
    byte: u8,
) -> Result<(), EspIdfError> {
    let bus = open_master_bus(sda_gpio, scl_gpio)?;
    let dev_cfg = i2c_device_config_t {
        dev_addr_length: 0, // I2C_ADDR_BIT_LEN_7
        device_address: u16::from(addr),
        scl_speed_hz: 100_000,
        scl_wait_us: 0,
        flags: Default::default(),
    };
    let mut dev_handle: i2c_master_dev_handle_t = core::ptr::null_mut();
    let status = unsafe { sys::i2c_master_bus_add_device(bus, &dev_cfg, &mut dev_handle) };
    if status != sys::ESP_OK as esp_err_t {
        return Err(EspIdfError::Io(format!(
            "i2c_master_bus_add_device(0x{:02x}) failed: {}",
            addr, status
        )));
    }
    let buf = [byte];
    let status = unsafe { i2c_master_transmit(dev_handle, buf.as_ptr(), buf.len(), 100) };
    if status != sys::ESP_OK as esp_err_t {
        return Err(EspIdfError::Io(format!(
            "i2c_master_transmit(0x{:02x}, 0x{:02x}) failed: {}",
            addr, byte, status
        )));
    }
    log::info!("i2c: wrote 0x{:02x} → device 0x{:02x}", byte, addr);
    Ok(())
}

fn open_master_bus(
    sda_gpio: i32,
    scl_gpio: i32,
) -> Result<i2c_master_bus_handle_t, EspIdfError> {
    // We only ever open one master bus during bring-up; if this is
    // called twice, i2c_new_master_bus returns ESP_ERR_INVALID_STATE.
    // Tolerate it by treating "already installed" as "reuse": we can't
    // actually recover the prior handle via the new-driver API, so the
    // caller must funnel all probes through one invocation. For the
    // one-shot scan use case we're in, that holds.
    let bus_cfg = i2c_master_bus_config_t {
        i2c_port: i2c_port_t_I2C_NUM_0 as i32,
        sda_io_num: sda_gpio,
        scl_io_num: scl_gpio,
        __bindgen_anon_1: i2c_master_bus_config_t__bindgen_ty_1 {
            clk_source: soc_periph_i2c_clk_src_t_I2C_CLK_SRC_DEFAULT as i2c_clock_source_t,
        },
        glitch_ignore_cnt: 7,
        intr_priority: 0,
        trans_queue_depth: 0,
        flags: i2c_master_bus_config_t__bindgen_ty_2 {
            _bitfield_align_1: [],
            _bitfield_1: {
                let mut bitfield = Default::default();
                {
                    let mut flags = i2c_master_bus_config_t__bindgen_ty_2 {
                        _bitfield_align_1: [],
                        _bitfield_1: Default::default(),
                        __bindgen_padding_0: [0; 3],
                    };
                    flags.set_enable_internal_pullup(1);
                    bitfield = flags._bitfield_1;
                }
                bitfield
            },
            __bindgen_padding_0: [0; 3],
        },
    };
    let mut bus_handle: i2c_master_bus_handle_t = core::ptr::null_mut();
    let status = unsafe { i2c_new_master_bus(&bus_cfg, &mut bus_handle) };
    if status != sys::ESP_OK as esp_err_t {
        return Err(EspIdfError::Io(format!(
            "i2c_new_master_bus(sda={}, scl={}) failed: {}",
            sda_gpio, scl_gpio, status
        )));
    }
    Ok(bus_handle)
}
