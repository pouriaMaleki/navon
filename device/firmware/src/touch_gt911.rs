//! Waveshare ESP32-P4-WIFI6-Touch-LCD-3.4C touch input via the upstream
//! `espressif/esp_lcd_touch_gt911` managed component.
//!
//! Same pattern as `mipi_dsi.rs`: pull in the C driver via
//! `package.metadata.esp-idf-sys.extra_components`, expose its symbols
//! through `gt911_bindings.h`, and wrap the handle in a Rust `TouchSource`
//! that reads the latched coordinates each frame.
//!
//! Wiring (from `waveshare/esp32_p4_wifi6_touch_lcd_xc` BSP):
//!   * I2C0  SDA = GPIO7,  SCL = GPIO8 — shared with the panel's CH422G
//!     (this is the same bus `i2c_scan` already uses).
//!   * Touch reset = GPIO23, active-low.
//!   * Touch INT  = unconnected. Both the BSP and Waveshare's reference
//!     board leave INT floating so address selection settles to the
//!     default 0x5D at power-on.
//!   * Polled, not interrupt-driven: we call `esp_lcd_touch_read_data`
//!     every frame (~16 ms) which is well under the GT911's 5 ms scan
//!     period, so contacts are reported with at most one frame of lag.

#![cfg(target_os = "espidf")]

use esp_idf_svc::sys::{
    self, esp_err_t, esp_lcd_new_panel_io_i2c_v1, esp_lcd_panel_io_handle_t,
    esp_lcd_panel_io_i2c_config_t, esp_lcd_touch_config_t, esp_lcd_touch_get_coordinates,
    esp_lcd_touch_handle_t, esp_lcd_touch_new_i2c_gt911, esp_lcd_touch_read_data,
    gpio_num_t_GPIO_NUM_NC, i2c_port_t_I2C_NUM_0,
};

use crate::board_config::TouchControllerConfig;
use crate::esp_idf::EspIdfError;
use crate::i2c_scan;
use crate::touch::{Gt9271ContactRecord, TouchContactState, TouchInput, TouchSource, TouchError};

/// Touch reset pin per the Waveshare 3.4C schematic / BSP. Active-low.
pub const WAVESHARE_3P4C_TOUCH_RESET_GPIO: i32 = 23;

/// Polled GT911 driver. Owns the C-side panel-IO + touch handle and uses
/// the same `TouchContactState` derivation as the existing GT9271 polling
/// source so the runtime sees identical Started/Moved/Stationary/Ended
/// frame semantics regardless of which controller is on the bus.
pub struct Gt911TouchSource {
    config: TouchControllerConfig,
    handle: esp_lcd_touch_handle_t,
    state: TouchContactState,
}

// SAFETY: the underlying `esp_lcd_touch_t` lives in PSRAM/heap and is
// only ever accessed through `&mut self`; the driver itself is single
// -threaded by design (it owns its own I2C panel-IO). Sending the handle
// across threads is fine; we never alias it.
unsafe impl Send for Gt911TouchSource {}

impl Gt911TouchSource {
    pub fn new(config: TouchControllerConfig) -> Result<Self, EspIdfError> {
        // 1. Make sure the legacy I2C master driver is up. Reuses the
        //    same install path the bus scanner uses — idempotent, so
        //    safe to call again on warm boots.
        i2c_scan::install_legacy_master_if_needed(
            i32::from(config.i2c_sda_gpio),
            i32::from(config.i2c_scl_gpio),
        )?;

        // 2. Create the I2C panel-IO. Values mirror
        //    `ESP_LCD_TOUCH_IO_I2C_GT911_CONFIG()` from the upstream
        //    header — the GT911's I2C protocol uses 16-bit register
        //    addresses with no D/C control phase.
        let mut io: esp_lcd_panel_io_handle_t = core::ptr::null_mut();
        let mut io_config = esp_lcd_panel_io_i2c_config_t::default();
        io_config.dev_addr = u32::from(config.address);
        // `scl_speed_hz` is rejected by the legacy I²C panel-IO driver
        // (`esp_lcd_new_panel_io_i2c_v1`) — speed is inherited from the
        // master `i2c_param_config` install in `i2c_scan`. Only the new
        // `_v2` API on top of `i2c_master_bus_handle_t` honors it.
        io_config.control_phase_bytes = 1;
        io_config.dc_bit_offset = 0;
        io_config.lcd_cmd_bits = 16;
        io_config.lcd_param_bits = 8;
        io_config.flags.set_disable_control_phase(1);

        check(
            unsafe {
                esp_lcd_new_panel_io_i2c_v1(i2c_port_t_I2C_NUM_0 as u32, &io_config, &mut io)
            },
            "esp_lcd_new_panel_io_i2c_v1",
        )?;

        // 3. Build the GT911 touch handle. Reset GPIO is wired to GPIO23
        //    on the 3.4C; INT is unconnected — the driver tolerates
        //    GPIO_NUM_NC and falls back to polled reads.
        let mut tp_config = esp_lcd_touch_config_t::default();
        tp_config.x_max = u16::from(config.controller_max_x) + 1;
        tp_config.y_max = u16::from(config.controller_max_y) + 1;
        tp_config.rst_gpio_num = config
            .rst_gpio
            .map(|g| g as i32)
            .unwrap_or(WAVESHARE_3P4C_TOUCH_RESET_GPIO);
        tp_config.int_gpio_num = config
            .int_gpio
            .map(|g| g as i32)
            .unwrap_or(gpio_num_t_GPIO_NUM_NC);
        tp_config.levels.set_reset(0);
        tp_config.levels.set_interrupt(0);

        let mut handle: esp_lcd_touch_handle_t = core::ptr::null_mut();
        check(
            unsafe { esp_lcd_touch_new_i2c_gt911(io, &tp_config, &mut handle) },
            "esp_lcd_touch_new_i2c_gt911",
        )?;

        log::info!(
            "gt911: touch ready, addr=0x{:02x}, rst_gpio={}, scl={}/sda={} (legacy I2C @ 100 kHz)",
            config.address,
            tp_config.rst_gpio_num,
            config.i2c_scl_gpio,
            config.i2c_sda_gpio,
        );

        Ok(Self {
            config,
            handle,
            state: TouchContactState::default(),
        })
    }
}

impl TouchSource for Gt911TouchSource {
    fn poll(&mut self) -> Result<Option<TouchInput>, TouchError> {
        // Step 1: latch the latest controller state into the driver's
        // internal `data` buffer. This issues the I2C reads.
        let status = unsafe { esp_lcd_touch_read_data(self.handle) };
        if status != sys::ESP_OK as esp_err_t {
            return Err(TouchError::Controller(format!(
                "esp_lcd_touch_read_data failed: {status}"
            )));
        }

        // Step 2: pull the latched coordinates out. esp_lcd_touch 1.1.x
        // doesn't yet expose per-contact track IDs (added in 1.2), so we
        // fall back to using the GT911's report-order slot index as the
        // ID — the controller keeps a finger in the same slot for the
        // duration of its contact, which is enough for our state machine
        // to derive Started → Moved/Stationary → Ended phases. For
        // multi-finger pinch-zoom we'd want real track IDs, but the
        // single-finger pan/tap path the runtime cares about today is
        // unaffected.
        let max_points = usize::from(self.config.max_contacts).min(5);
        let mut xs = vec![0u16; max_points];
        let mut ys = vec![0u16; max_points];
        let mut strengths = vec![0u16; max_points];
        let mut point_cnt: u8 = 0;
        let _touched = unsafe {
            esp_lcd_touch_get_coordinates(
                self.handle,
                xs.as_mut_ptr(),
                ys.as_mut_ptr(),
                strengths.as_mut_ptr(),
                &mut point_cnt,
                max_points as u8,
            )
        };
        let active: Vec<Gt9271ContactRecord> = (0..usize::from(point_cnt))
            .map(|i| Gt9271ContactRecord {
                track_id: i as u8,
                x: xs[i],
                y: ys[i],
                size: strengths[i],
            })
            .collect();

        Ok(self.state.update(self.config, active))
    }
}

fn check(status: esp_err_t, op: &str) -> Result<(), EspIdfError> {
    if status == sys::ESP_OK as esp_err_t {
        Ok(())
    } else {
        Err(EspIdfError::Io(format!("{op} failed: {status}")))
    }
}
