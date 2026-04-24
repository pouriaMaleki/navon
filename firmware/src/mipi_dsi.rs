//! ESP-IDF MIPI-DSI panel driver for the Waveshare ESP32-P4 family.
//!
//! Wraps the C APIs in `components/esp_lcd/dsi/` (`esp_lcd_new_dsi_bus`,
//! `esp_lcd_new_panel_io_dbi`, `esp_lcd_new_panel_dpi`) behind the
//! `EspIdfPanel` trait so the firmware `Display` backend can drive a real
//! MIPI-DSI panel on device.
//!
//! ## Scope (Phase 3b)
//!
//! - Bring-up: DSI PHY, DBI (command) channel, DPI (pixel stream) channel.
//! - Configurable video timing + vendor init sequence so the same driver
//!   can be reused across Waveshare SKUs (7″ 1024×600 JT070L1B, 3.4″ 800×800,
//!   etc.) without code changes.
//! - RGB565 pixel path (matches `PanelPixelFormat::Rgb565Le` used by the
//!   software rasterizer).
//!
//! ## Out of scope (Phase 3b)
//!
//! - Touch controller wiring (GT911 / CST) — lives in `touch.rs` and is
//!   per-SKU.
//! - Backlight PWM / reset GPIOs — wired by the boot code once the exact
//!   panel is confirmed.
//! - DMA2D acceleration — the `esp_lcd_dpi_panel_config_t::flags.use_dma2d`
//!   switch is exposed in [`PanelTiming`] but off by default.

#![cfg(target_os = "espidf")]

use core::ffi::c_void;
use core::ptr;

use esp_idf_svc::sys::{
    self, esp_err_t, esp_ldo_acquire_channel, esp_ldo_channel_config_t,
    esp_ldo_channel_handle_t, esp_lcd_dbi_io_config_t, esp_lcd_dpi_panel_config_t,
    esp_lcd_dpi_panel_config_t_extra_dpi_panel_flags, esp_lcd_dsi_bus_config_t,
    esp_lcd_dsi_bus_handle_t, esp_lcd_new_dsi_bus, esp_lcd_new_panel_io_dbi, esp_lcd_panel_del,
    esp_lcd_panel_handle_t, esp_lcd_panel_init, esp_lcd_panel_io_handle_t,
    esp_lcd_panel_io_tx_param, esp_lcd_panel_reset, esp_lcd_video_timing_t,
    // Enum values come through bindgen as namespaced constants, not as
    // associated items on the type alias.
    gpio_config_t, gpio_int_type_t_GPIO_INTR_DISABLE, gpio_mode_t_GPIO_MODE_OUTPUT,
    gpio_pulldown_t_GPIO_PULLDOWN_DISABLE, gpio_pullup_t_GPIO_PULLUP_DISABLE, gpio_set_level,
    lcd_color_format_t_LCD_COLOR_FMT_RGB565,
    lcd_color_rgb_pixel_format_t_LCD_COLOR_PIXEL_FORMAT_RGB565,
    soc_periph_mipi_dsi_dpi_clk_src_t_MIPI_DSI_DPI_CLK_SRC_DEFAULT,
    soc_periph_mipi_dsi_phy_clk_src_t_MIPI_DSI_PHY_CLK_SRC_DEFAULT,
};

use crate::board_config::{DisplayConfig, TouchControllerConfig};
use crate::esp_idf::{EspIdfError, EspIdfPanel};

/// A single MIPI-DSI Display-Command-Set packet to send at init time.
#[derive(Debug, Clone)]
pub struct InitCommand {
    /// DCS command byte (e.g. `0x11` SLPOUT, `0x29` DISPON).
    pub cmd: u8,
    /// Parameter bytes. Empty for parameter-less commands.
    pub params: &'static [u8],
    /// Milliseconds to wait after sending the command. `0` for no delay.
    pub delay_ms: u32,
}

/// Video timing for the MIPI-DSI DPI panel. These values come from the panel
/// datasheet; keeping them in data rather than code lets the same driver
/// serve different SKUs (7″ 1024×600 vs 3.4″ 800×800 etc.).
#[derive(Debug, Clone, Copy)]
pub struct PanelTiming {
    pub h_active: u32,
    pub v_active: u32,
    pub h_sync_pulse_width: u32,
    pub h_back_porch: u32,
    pub h_front_porch: u32,
    pub v_sync_pulse_width: u32,
    pub v_back_porch: u32,
    pub v_front_porch: u32,
    /// DPI clock frequency in MHz.
    pub dpi_clock_mhz: u32,
    /// Per-lane bit rate in Mbps.
    pub lane_bit_rate_mbps: u32,
    /// Number of DSI data lanes (1 or 2 on P4).
    pub num_data_lanes: u8,
}

/// Full MIPI-DSI panel configuration: timing + init sequence.
#[derive(Debug, Clone)]
pub struct MipiDsiConfig {
    pub timing: PanelTiming,
    pub init_sequence: &'static [InitCommand],
}

/// Concrete `EspIdfPanel` that drives a MIPI-DSI display.
#[derive(Debug)]
pub struct MipiDsiPanel {
    config: MipiDsiConfig,
    bus: esp_lcd_dsi_bus_handle_t,
    dbi_io: esp_lcd_panel_io_handle_t,
    dpi_panel: esp_lcd_panel_handle_t,
    initialized: bool,
}

impl MipiDsiPanel {
    /// Allocate the DSI bus and command IO. The DPI (pixel stream) panel is
    /// created lazily in [`EspIdfPanel::initialize`] because it needs the
    /// framebuffer viewport dimensions, which are part of `DisplayConfig`.
    pub fn new(config: MipiDsiConfig) -> Result<Self, EspIdfError> {
        unsafe {
            let bus_config = esp_lcd_dsi_bus_config_t {
                bus_id: 0,
                num_data_lanes: config.timing.num_data_lanes,
                phy_clk_src: soc_periph_mipi_dsi_phy_clk_src_t_MIPI_DSI_PHY_CLK_SRC_DEFAULT,
                lane_bit_rate_mbps: config.timing.lane_bit_rate_mbps,
            };
            let mut bus: esp_lcd_dsi_bus_handle_t = ptr::null_mut();
            check(esp_lcd_new_dsi_bus(&bus_config, &mut bus))?;

            let dbi_io_config = esp_lcd_dbi_io_config_t {
                virtual_channel: 0,
                lcd_cmd_bits: 8,
                lcd_param_bits: 8,
            };
            let mut dbi_io: esp_lcd_panel_io_handle_t = ptr::null_mut();
            check(esp_lcd_new_panel_io_dbi(bus, &dbi_io_config, &mut dbi_io))?;

            Ok(Self {
                config,
                bus,
                dbi_io,
                dpi_panel: ptr::null_mut(),
                initialized: false,
            })
        }
    }

    fn send_init_sequence(&mut self) -> Result<(), EspIdfError> {
        for step in self.config.init_sequence {
            unsafe {
                let status = esp_lcd_panel_io_tx_param(
                    self.dbi_io,
                    step.cmd as i32,
                    step.params.as_ptr().cast::<c_void>(),
                    step.params.len(),
                );
                check(status)?;
            }
            if step.delay_ms > 0 {
                unsafe {
                    sys::vTaskDelay(step.delay_ms / 10);
                }
            }
        }
        Ok(())
    }
}

impl Drop for MipiDsiPanel {
    fn drop(&mut self) {
        unsafe {
            if !self.dpi_panel.is_null() {
                let _ = esp_lcd_panel_del(self.dpi_panel);
            }
            if !self.dbi_io.is_null() {
                let _ = sys::esp_lcd_panel_io_del(self.dbi_io);
            }
            if !self.bus.is_null() {
                let _ = sys::esp_lcd_del_dsi_bus(self.bus);
            }
        }
    }
}

impl EspIdfPanel for MipiDsiPanel {
    fn initialize(&mut self, config: DisplayConfig) -> Result<(), EspIdfError> {
        if self.initialized {
            return Ok(());
        }

        // Build the DPI (pixel stream) panel for this viewport. On Phase 3
        // the viewport_size already matches the panel's native resolution
        // via `board_config::PanelTiming`; we cross-check here.
        let timing = self.config.timing;
        if config.viewport_size.width_px != timing.h_active
            || config.viewport_size.height_px != timing.v_active
        {
            return Err(EspIdfError::Unsupported(format!(
                "mipi-dsi panel timing {}x{} does not match DisplayConfig viewport {}x{}",
                timing.h_active,
                timing.v_active,
                config.viewport_size.width_px,
                config.viewport_size.height_px,
            )));
        }

        unsafe {
            let video_timing = esp_lcd_video_timing_t {
                h_size: timing.h_active,
                v_size: timing.v_active,
                hsync_back_porch: timing.h_back_porch,
                hsync_pulse_width: timing.h_sync_pulse_width,
                hsync_front_porch: timing.h_front_porch,
                vsync_back_porch: timing.v_back_porch,
                vsync_pulse_width: timing.v_sync_pulse_width,
                vsync_front_porch: timing.v_front_porch,
            };
            let dpi_config = esp_lcd_dpi_panel_config_t {
                virtual_channel: 0,
                dpi_clk_src: soc_periph_mipi_dsi_dpi_clk_src_t_MIPI_DSI_DPI_CLK_SRC_DEFAULT,
                dpi_clock_freq_mhz: timing.dpi_clock_mhz,
                pixel_format: lcd_color_rgb_pixel_format_t_LCD_COLOR_PIXEL_FORMAT_RGB565,
                in_color_format: lcd_color_format_t_LCD_COLOR_FMT_RGB565,
                out_color_format: lcd_color_format_t_LCD_COLOR_FMT_RGB565,
                num_fbs: 1,
                video_timing,
                flags: esp_lcd_dpi_panel_config_t_extra_dpi_panel_flags::default(),
            };
            let mut panel: esp_lcd_panel_handle_t = ptr::null_mut();
            check(sys::esp_lcd_new_panel_dpi(self.bus, &dpi_config, &mut panel))?;
            self.dpi_panel = panel;

            check(esp_lcd_panel_reset(self.dpi_panel))?;
            check(esp_lcd_panel_init(self.dpi_panel))?;
        }

        self.send_init_sequence()?;

        self.initialized = true;
        Ok(())
    }

    fn present(
        &mut self,
        pixels: &[u8],
        width: u32,
        height: u32,
        _config: DisplayConfig,
    ) -> Result<(), EspIdfError> {
        if !self.initialized {
            return Err(EspIdfError::Unsupported(
                "mipi-dsi panel presented before init".to_owned(),
            ));
        }
        unsafe {
            check(sys::esp_lcd_panel_draw_bitmap(
                self.dpi_panel,
                0,
                0,
                width as i32,
                height as i32,
                pixels.as_ptr().cast::<c_void>(),
            ))?;
        }
        Ok(())
    }
}

/// Waveshare ESP32-P4-WIFI6-Touch-LCD-3.4C — 3.4″ round 800×800 MIPI-DSI
/// panel with an ST77922 driver IC, on the same carrier board as the P4.
/// Pin wiring (reset, backlight) comes from the Waveshare schematic; the
/// init sequence is the minimum known-good subset (sleep-out + display-on)
/// plus the pixel-format / tearing-effect settings that the Waveshare
/// demo firmware ships. Gamma / VCOM tuning is deferred — the panel
/// should light up with legible (if slightly off-color) output at this
/// level, which is enough to prove the full pipeline on device.
pub fn waveshare_3p4c_st77922_config() -> MipiDsiConfig {
    MipiDsiConfig {
        timing: PanelTiming {
            h_active: 800,
            v_active: 800,
            // Values from Waveshare's ESP-IDF sample for the 3.4C kit.
            // Conservative porches; the ST77922 is tolerant here.
            h_sync_pulse_width: 20,
            h_back_porch: 20,
            h_front_porch: 40,
            v_sync_pulse_width: 4,
            v_back_porch: 12,
            v_front_porch: 20,
            dpi_clock_mhz: 48,
            lane_bit_rate_mbps: 1000,
            num_data_lanes: 2,
        },
        init_sequence: &[
            // Sitronix command-set select page 0 (most ST77922 registers
            // live here; some panels omit this — kept for robustness).
            InitCommand {
                cmd: 0xFE,
                params: &[0x00],
                delay_ms: 0,
            },
            // Tearing effect line enabled, vsync-only.
            InitCommand {
                cmd: 0x35,
                params: &[0x00],
                delay_ms: 0,
            },
            // Pixel format: 16 bpp RGB565 on DBI / RGB565 on DPI.
            InitCommand {
                cmd: 0x3A,
                params: &[0x55],
                delay_ms: 0,
            },
            // Sleep out.
            InitCommand {
                cmd: 0x11,
                params: &[],
                delay_ms: 120,
            },
            // Display on.
            InitCommand {
                cmd: 0x29,
                params: &[],
                delay_ms: 20,
            },
        ],
    }
}

/// Default reset / backlight GPIO mapping for the Waveshare
/// ESP32-P4-WIFI6-Touch-LCD-3.4C carrier. These are the values the
/// Waveshare reference firmware uses; if the board in hand disagrees,
/// adjust via [`PanelGpios`] at construction time.
pub const WAVESHARE_3P4C_RESET_GPIO: i32 = 27;
pub const WAVESHARE_3P4C_BACKLIGHT_GPIO: i32 = 26;

/// GPIOs for the panel's out-of-band control lines. Values are
/// `gpio_num_t` (i32); `None` means the line is not connected.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PanelGpios {
    pub reset: Option<i32>,
    pub backlight: Option<i32>,
}

impl PanelGpios {
    pub const WAVESHARE_3P4C: Self = Self {
        reset: Some(WAVESHARE_3P4C_RESET_GPIO),
        backlight: Some(WAVESHARE_3P4C_BACKLIGHT_GPIO),
    };
}

/// Acquire the on-chip LDO channel that powers the MIPI-DSI PHY. On the
/// ESP32-P4 this is channel 3 at 2500 mV; the DSI PHY will not lock
/// without it. The returned handle must be kept alive for the lifetime
/// of the DSI bus — dropping it powers the rail down.
pub fn acquire_mipi_dsi_phy_power() -> Result<esp_ldo_channel_handle_t, EspIdfError> {
    let cfg = esp_ldo_channel_config_t {
        chan_id: 3,
        voltage_mv: 2500,
        flags: Default::default(),
    };
    let mut handle: esp_ldo_channel_handle_t = core::ptr::null_mut();
    let status = unsafe { esp_ldo_acquire_channel(&cfg, &mut handle) };
    if status != sys::ESP_OK as esp_err_t {
        return Err(EspIdfError::Io(format!(
            "esp_ldo_acquire_channel(3, 2500mV) failed: {}",
            status
        )));
    }
    Ok(handle)
}

/// Drive a GPIO low then high with the delays commonly required by
/// MIPI-DSI panels after power-up. Call before `MipiDsiPanel::initialize`.
pub fn pulse_reset(gpio: i32) -> Result<(), EspIdfError> {
    configure_output(gpio)?;
    unsafe {
        set_level(gpio, 1)?;
        sys::vTaskDelay(10 / 10); // 10 ms
        set_level(gpio, 0)?;
        sys::vTaskDelay(10 / 10); // 10 ms
        set_level(gpio, 1)?;
        sys::vTaskDelay(120 / 10); // 120 ms settle
    }
    Ok(())
}

/// Enable the panel backlight by driving its GPIO high. Power budget on
/// the 3.4C module is low enough that a single-GPIO always-on is fine
/// for bring-up; PWM dimming can be layered on later.
pub fn enable_backlight(gpio: i32) -> Result<(), EspIdfError> {
    configure_output(gpio)?;
    unsafe {
        set_level(gpio, 1)?;
    }
    Ok(())
}

fn configure_output(gpio: i32) -> Result<(), EspIdfError> {
    let config = gpio_config_t {
        pin_bit_mask: 1u64 << gpio,
        mode: gpio_mode_t_GPIO_MODE_OUTPUT,
        pull_up_en: gpio_pullup_t_GPIO_PULLUP_DISABLE,
        pull_down_en: gpio_pulldown_t_GPIO_PULLDOWN_DISABLE,
        intr_type: gpio_int_type_t_GPIO_INTR_DISABLE,
        ..gpio_config_t::default()
    };
    let status = unsafe { sys::gpio_config(&config) };
    if status != sys::ESP_OK as esp_err_t {
        return Err(EspIdfError::Io(format!(
            "gpio_config({}) failed: {}",
            gpio, status
        )));
    }
    Ok(())
}

unsafe fn set_level(gpio: i32, level: u32) -> Result<(), EspIdfError> {
    let status = unsafe { gpio_set_level(gpio, level) };
    if status != sys::ESP_OK as esp_err_t {
        return Err(EspIdfError::Io(format!(
            "gpio_set_level({}, {}) failed: {}",
            gpio, level, status
        )));
    }
    Ok(())
}

/// Convenience factory for the Waveshare 1024×600 7″ panel (JT070L1B).
/// **Timing values below are defaults from the ESP-IDF `mipi_dsi_lcd`
/// test-app and will likely need tweaking for the specific panel SKU** —
/// especially `h_sync_pulse_width`, `h_back_porch`, `dpi_clock_mhz`. Keep
/// them here as a starting point; replace when you have the panel datasheet.
pub fn waveshare_7in_jt070l1b_config() -> MipiDsiConfig {
    MipiDsiConfig {
        timing: PanelTiming {
            h_active: 1024,
            v_active: 600,
            h_sync_pulse_width: 10,
            h_back_porch: 160,
            h_front_porch: 160,
            v_sync_pulse_width: 1,
            v_back_porch: 23,
            v_front_porch: 12,
            dpi_clock_mhz: 52,
            lane_bit_rate_mbps: 1000,
            num_data_lanes: 2,
        },
        init_sequence: &[
            InitCommand {
                cmd: 0x11,
                params: &[],
                delay_ms: 120,
            }, // SLPOUT
            InitCommand {
                cmd: 0x29,
                params: &[],
                delay_ms: 20,
            }, // DISPON
        ],
    }
}

/// Convenience factory for a generic 800×800 square panel. Same caveat as
/// [`waveshare_7in_jt070l1b_config`] — timing values are a placeholder
/// sanity default, not a datasheet-driven sequence.
pub fn generic_800x800_config() -> MipiDsiConfig {
    MipiDsiConfig {
        timing: PanelTiming {
            h_active: 800,
            v_active: 800,
            h_sync_pulse_width: 10,
            h_back_porch: 20,
            h_front_porch: 20,
            v_sync_pulse_width: 2,
            v_back_porch: 8,
            v_front_porch: 8,
            dpi_clock_mhz: 40,
            lane_bit_rate_mbps: 1000,
            num_data_lanes: 2,
        },
        init_sequence: &[
            InitCommand {
                cmd: 0x11,
                params: &[],
                delay_ms: 120,
            },
            InitCommand {
                cmd: 0x29,
                params: &[],
                delay_ms: 20,
            },
        ],
    }
}

fn check(status: esp_err_t) -> Result<(), EspIdfError> {
    if status == sys::ESP_OK as esp_err_t {
        Ok(())
    } else {
        Err(EspIdfError::Io(format!("esp_lcd call failed: {}", status)))
    }
}

// Touch pin / bus helpers kept next to the panel driver because panel and
// touch are usually on the same FPC and share reset/interrupt GPIOs on
// Waveshare boards. Concrete I2C wiring still lives in `touch.rs`.
#[allow(dead_code)]
pub fn touch_defaults_matching_panel(display: &DisplayConfig) -> TouchControllerConfig {
    TouchControllerConfig {
        logical_width_px: display.viewport_size.width_px,
        logical_height_px: display.viewport_size.height_px,
        ..TouchControllerConfig::default()
    }
}
