//! ESP-IDF MIPI-DSI panel driver wired for the Waveshare
//! ESP32-P4-WIFI6-Touch-LCD-3.4C — 3.4″ round 800×800 MIPI-DSI panel
//! built around a JD9365 driver IC.
//!
//! The panel needs a ~200-command vendor init sequence to actually
//! produce pixels. Rather than re-implement all that by hand we pull
//! `espressif/esp_lcd_jd9365` from the ESP Component Registry (added
//! as a managed component in [firmware/Cargo.toml](../Cargo.toml)) and
//! feed it our extracted Waveshare init array. The driver handles
//! reset, init-command transmission, and DPI panel creation internally.

#![cfg(target_os = "espidf")]

use core::ffi::{c_int, c_uint, c_void};
use core::ptr;

use esp_idf_svc::sys::{
    self, esp_err_t, esp_ldo_acquire_channel, esp_ldo_channel_config_t,
    esp_ldo_channel_handle_t, esp_lcd_dbi_io_config_t, esp_lcd_dpi_panel_config_t,
    esp_lcd_dpi_panel_config_t_extra_dpi_panel_flags, esp_lcd_dsi_bus_config_t,
    esp_lcd_dsi_bus_handle_t, esp_lcd_new_dsi_bus, esp_lcd_new_panel_io_dbi,
    esp_lcd_new_panel_jd9365, esp_lcd_panel_del, esp_lcd_panel_dev_config_t,
    esp_lcd_panel_handle_t, esp_lcd_panel_init, esp_lcd_panel_io_handle_t,
    esp_lcd_panel_reset, esp_lcd_video_timing_t, gpio_config_t,
    gpio_int_type_t_GPIO_INTR_DISABLE, gpio_mode_t_GPIO_MODE_OUTPUT,
    gpio_pulldown_t_GPIO_PULLDOWN_DISABLE, gpio_pullup_t_GPIO_PULLUP_DISABLE,
    gpio_set_level, jd9365_lcd_init_cmd_t, jd9365_vendor_config_t,
    lcd_color_format_t_LCD_COLOR_FMT_RGB565,
    lcd_color_rgb_pixel_format_t_LCD_COLOR_PIXEL_FORMAT_RGB565,
    soc_periph_mipi_dsi_dpi_clk_src_t_MIPI_DSI_DPI_CLK_SRC_DEFAULT,
    soc_periph_mipi_dsi_phy_clk_src_t_MIPI_DSI_PHY_CLK_SRC_DEFAULT,
};

use crate::board_config::{DisplayConfig, TouchControllerConfig};
use crate::esp_idf::{EspIdfError, EspIdfPanel};

/// One JD9365 init step: a command byte plus zero or more parameter
/// bytes plus an optional post-command delay. Stored Rust-side; we
/// project this into the C `jd9365_lcd_init_cmd_t` shape at runtime.
#[derive(Debug, Clone, Copy)]
pub struct Jd9365Cmd {
    pub cmd: u8,
    pub params: &'static [u8],
    pub delay_ms: u32,
}

impl Jd9365Cmd {
    pub const fn new(cmd: u8, params: &'static [u8], delay_ms: u32) -> Self {
        Self { cmd, params, delay_ms }
    }
}

/// Waveshare 3.4″ round 800×800 (CONFIG_BSP_LCD_TYPE_800_800_3_4_INCH)
/// JD9365 init sequence, lifted verbatim from the
/// `waveshare/esp32_p4_wifi6_touch_lcd_xc` BSP. Do not edit individual
/// values without consulting the panel datasheet — these are gamma /
/// VCOM / source-driver settings that the panel needs in this exact
/// order to produce a usable image.
pub const WAVESHARE_3P4C_INIT_CMDS: &[Jd9365Cmd] = &[
    Jd9365Cmd::new(0xE0, &[0x00], 0),
    Jd9365Cmd::new(0xE1, &[0x93], 0),
    Jd9365Cmd::new(0xE2, &[0x65], 0),
    Jd9365Cmd::new(0xE3, &[0xF8], 0),
    Jd9365Cmd::new(0x80, &[0x01], 0),
    Jd9365Cmd::new(0xE0, &[0x01], 0),
    Jd9365Cmd::new(0x00, &[0x00], 0),
    Jd9365Cmd::new(0x01, &[0x41], 0),
    Jd9365Cmd::new(0x03, &[0x10], 0),
    Jd9365Cmd::new(0x04, &[0x44], 0),
    Jd9365Cmd::new(0x17, &[0x00], 0),
    Jd9365Cmd::new(0x18, &[0xD0], 0),
    Jd9365Cmd::new(0x19, &[0x00], 0),
    Jd9365Cmd::new(0x1A, &[0x00], 0),
    Jd9365Cmd::new(0x1B, &[0xD0], 0),
    Jd9365Cmd::new(0x1C, &[0x00], 0),
    Jd9365Cmd::new(0x24, &[0xFE], 0),
    Jd9365Cmd::new(0x35, &[0x26], 0),
    Jd9365Cmd::new(0x37, &[0x09], 0),
    Jd9365Cmd::new(0x38, &[0x04], 0),
    Jd9365Cmd::new(0x39, &[0x08], 0),
    Jd9365Cmd::new(0x3A, &[0x0A], 0),
    Jd9365Cmd::new(0x3C, &[0x78], 0),
    Jd9365Cmd::new(0x3D, &[0xFF], 0),
    Jd9365Cmd::new(0x3E, &[0xFF], 0),
    Jd9365Cmd::new(0x3F, &[0xFF], 0),
    Jd9365Cmd::new(0x40, &[0x00], 0),
    Jd9365Cmd::new(0x41, &[0x64], 0),
    Jd9365Cmd::new(0x42, &[0xC7], 0),
    Jd9365Cmd::new(0x43, &[0x18], 0),
    Jd9365Cmd::new(0x44, &[0x0B], 0),
    Jd9365Cmd::new(0x45, &[0x14], 0),
    Jd9365Cmd::new(0x55, &[0x02], 0),
    Jd9365Cmd::new(0x57, &[0x49], 0),
    Jd9365Cmd::new(0x59, &[0x0A], 0),
    Jd9365Cmd::new(0x5A, &[0x1B], 0),
    Jd9365Cmd::new(0x5B, &[0x19], 0),
    Jd9365Cmd::new(0x5D, &[0x7F], 0),
    Jd9365Cmd::new(0x5E, &[0x56], 0),
    Jd9365Cmd::new(0x5F, &[0x43], 0),
    Jd9365Cmd::new(0x60, &[0x37], 0),
    Jd9365Cmd::new(0x61, &[0x33], 0),
    Jd9365Cmd::new(0x62, &[0x25], 0),
    Jd9365Cmd::new(0x63, &[0x2A], 0),
    Jd9365Cmd::new(0x64, &[0x16], 0),
    Jd9365Cmd::new(0x65, &[0x30], 0),
    Jd9365Cmd::new(0x66, &[0x2F], 0),
    Jd9365Cmd::new(0x67, &[0x32], 0),
    Jd9365Cmd::new(0x68, &[0x53], 0),
    Jd9365Cmd::new(0x69, &[0x43], 0),
    Jd9365Cmd::new(0x6A, &[0x4C], 0),
    Jd9365Cmd::new(0x6B, &[0x40], 0),
    Jd9365Cmd::new(0x6C, &[0x3D], 0),
    Jd9365Cmd::new(0x6D, &[0x31], 0),
    Jd9365Cmd::new(0x6E, &[0x20], 0),
    Jd9365Cmd::new(0x6F, &[0x0F], 0),
    Jd9365Cmd::new(0x70, &[0x7F], 0),
    Jd9365Cmd::new(0x71, &[0x56], 0),
    Jd9365Cmd::new(0x72, &[0x43], 0),
    Jd9365Cmd::new(0x73, &[0x37], 0),
    Jd9365Cmd::new(0x74, &[0x33], 0),
    Jd9365Cmd::new(0x75, &[0x25], 0),
    Jd9365Cmd::new(0x76, &[0x2A], 0),
    Jd9365Cmd::new(0x77, &[0x16], 0),
    Jd9365Cmd::new(0x78, &[0x30], 0),
    Jd9365Cmd::new(0x79, &[0x2F], 0),
    Jd9365Cmd::new(0x7A, &[0x32], 0),
    Jd9365Cmd::new(0x7B, &[0x53], 0),
    Jd9365Cmd::new(0x7C, &[0x43], 0),
    Jd9365Cmd::new(0x7D, &[0x4C], 0),
    Jd9365Cmd::new(0x7E, &[0x40], 0),
    Jd9365Cmd::new(0x7F, &[0x3D], 0),
    Jd9365Cmd::new(0x80, &[0x31], 0),
    Jd9365Cmd::new(0x81, &[0x20], 0),
    Jd9365Cmd::new(0x82, &[0x0F], 0),
    Jd9365Cmd::new(0xE0, &[0x02], 0),
    Jd9365Cmd::new(0x00, &[0x5F], 0),
    Jd9365Cmd::new(0x01, &[0x5F], 0),
    Jd9365Cmd::new(0x02, &[0x5E], 0),
    Jd9365Cmd::new(0x03, &[0x5E], 0),
    Jd9365Cmd::new(0x04, &[0x50], 0),
    Jd9365Cmd::new(0x05, &[0x48], 0),
    Jd9365Cmd::new(0x06, &[0x48], 0),
    Jd9365Cmd::new(0x07, &[0x4A], 0),
    Jd9365Cmd::new(0x08, &[0x4A], 0),
    Jd9365Cmd::new(0x09, &[0x44], 0),
    Jd9365Cmd::new(0x0A, &[0x44], 0),
    Jd9365Cmd::new(0x0B, &[0x46], 0),
    Jd9365Cmd::new(0x0C, &[0x46], 0),
    Jd9365Cmd::new(0x0D, &[0x5F], 0),
    Jd9365Cmd::new(0x0E, &[0x5F], 0),
    Jd9365Cmd::new(0x0F, &[0x57], 0),
    Jd9365Cmd::new(0x10, &[0x57], 0),
    Jd9365Cmd::new(0x11, &[0x77], 0),
    Jd9365Cmd::new(0x12, &[0x77], 0),
    Jd9365Cmd::new(0x13, &[0x40], 0),
    Jd9365Cmd::new(0x14, &[0x42], 0),
    Jd9365Cmd::new(0x15, &[0x5F], 0),
    Jd9365Cmd::new(0x16, &[0x5F], 0),
    Jd9365Cmd::new(0x17, &[0x5F], 0),
    Jd9365Cmd::new(0x18, &[0x5E], 0),
    Jd9365Cmd::new(0x19, &[0x5E], 0),
    Jd9365Cmd::new(0x1A, &[0x50], 0),
    Jd9365Cmd::new(0x1B, &[0x49], 0),
    Jd9365Cmd::new(0x1C, &[0x49], 0),
    Jd9365Cmd::new(0x1D, &[0x4B], 0),
    Jd9365Cmd::new(0x1E, &[0x4B], 0),
    Jd9365Cmd::new(0x1F, &[0x45], 0),
    Jd9365Cmd::new(0x20, &[0x45], 0),
    Jd9365Cmd::new(0x21, &[0x47], 0),
    Jd9365Cmd::new(0x22, &[0x47], 0),
    Jd9365Cmd::new(0x23, &[0x5F], 0),
    Jd9365Cmd::new(0x24, &[0x5F], 0),
    Jd9365Cmd::new(0x25, &[0x57], 0),
    Jd9365Cmd::new(0x26, &[0x57], 0),
    Jd9365Cmd::new(0x27, &[0x77], 0),
    Jd9365Cmd::new(0x28, &[0x77], 0),
    Jd9365Cmd::new(0x29, &[0x41], 0),
    Jd9365Cmd::new(0x2A, &[0x43], 0),
    Jd9365Cmd::new(0x2B, &[0x5F], 0),
    Jd9365Cmd::new(0x2C, &[0x1E], 0),
    Jd9365Cmd::new(0x2D, &[0x1E], 0),
    Jd9365Cmd::new(0x2E, &[0x1F], 0),
    Jd9365Cmd::new(0x2F, &[0x1F], 0),
    Jd9365Cmd::new(0x30, &[0x10], 0),
    Jd9365Cmd::new(0x31, &[0x07], 0),
    Jd9365Cmd::new(0x32, &[0x07], 0),
    Jd9365Cmd::new(0x33, &[0x05], 0),
    Jd9365Cmd::new(0x34, &[0x05], 0),
    Jd9365Cmd::new(0x35, &[0x0B], 0),
    Jd9365Cmd::new(0x36, &[0x0B], 0),
    Jd9365Cmd::new(0x37, &[0x09], 0),
    Jd9365Cmd::new(0x38, &[0x09], 0),
    Jd9365Cmd::new(0x39, &[0x1F], 0),
    Jd9365Cmd::new(0x3A, &[0x1F], 0),
    Jd9365Cmd::new(0x3B, &[0x17], 0),
    Jd9365Cmd::new(0x3C, &[0x17], 0),
    Jd9365Cmd::new(0x3D, &[0x17], 0),
    Jd9365Cmd::new(0x3E, &[0x17], 0),
    Jd9365Cmd::new(0x3F, &[0x03], 0),
    Jd9365Cmd::new(0x40, &[0x01], 0),
    Jd9365Cmd::new(0x41, &[0x1F], 0),
    Jd9365Cmd::new(0x42, &[0x1E], 0),
    Jd9365Cmd::new(0x43, &[0x1E], 0),
    Jd9365Cmd::new(0x44, &[0x1F], 0),
    Jd9365Cmd::new(0x45, &[0x1F], 0),
    Jd9365Cmd::new(0x46, &[0x10], 0),
    Jd9365Cmd::new(0x47, &[0x06], 0),
    Jd9365Cmd::new(0x48, &[0x06], 0),
    Jd9365Cmd::new(0x49, &[0x04], 0),
    Jd9365Cmd::new(0x4A, &[0x04], 0),
    Jd9365Cmd::new(0x4B, &[0x0A], 0),
    Jd9365Cmd::new(0x4C, &[0x0A], 0),
    Jd9365Cmd::new(0x4D, &[0x08], 0),
    Jd9365Cmd::new(0x4E, &[0x08], 0),
    Jd9365Cmd::new(0x4F, &[0x1F], 0),
    Jd9365Cmd::new(0x50, &[0x1F], 0),
    Jd9365Cmd::new(0x51, &[0x17], 0),
    Jd9365Cmd::new(0x52, &[0x17], 0),
    Jd9365Cmd::new(0x53, &[0x17], 0),
    Jd9365Cmd::new(0x54, &[0x17], 0),
    Jd9365Cmd::new(0x55, &[0x02], 0),
    Jd9365Cmd::new(0x56, &[0x00], 0),
    Jd9365Cmd::new(0x57, &[0x1F], 0),
    Jd9365Cmd::new(0xE0, &[0x02], 0),
    Jd9365Cmd::new(0x58, &[0x40], 0),
    Jd9365Cmd::new(0x59, &[0x00], 0),
    Jd9365Cmd::new(0x5A, &[0x00], 0),
    Jd9365Cmd::new(0x5B, &[0x30], 0),
    Jd9365Cmd::new(0x5C, &[0x01], 0),
    Jd9365Cmd::new(0x5D, &[0x30], 0),
    Jd9365Cmd::new(0x5E, &[0x01], 0),
    Jd9365Cmd::new(0x5F, &[0x02], 0),
    Jd9365Cmd::new(0x60, &[0x30], 0),
    Jd9365Cmd::new(0x61, &[0x03], 0),
    Jd9365Cmd::new(0x62, &[0x04], 0),
    Jd9365Cmd::new(0x63, &[0x04], 0),
    Jd9365Cmd::new(0x64, &[0xA6], 0),
    Jd9365Cmd::new(0x65, &[0x43], 0),
    Jd9365Cmd::new(0x66, &[0x30], 0),
    Jd9365Cmd::new(0x67, &[0x73], 0),
    Jd9365Cmd::new(0x68, &[0x05], 0),
    Jd9365Cmd::new(0x69, &[0x04], 0),
    Jd9365Cmd::new(0x6A, &[0x7F], 0),
    Jd9365Cmd::new(0x6B, &[0x08], 0),
    Jd9365Cmd::new(0x6C, &[0x00], 0),
    Jd9365Cmd::new(0x6D, &[0x04], 0),
    Jd9365Cmd::new(0x6E, &[0x04], 0),
    Jd9365Cmd::new(0x6F, &[0x88], 0),
    Jd9365Cmd::new(0x75, &[0xD9], 0),
    Jd9365Cmd::new(0x76, &[0x00], 0),
    Jd9365Cmd::new(0x77, &[0x33], 0),
    Jd9365Cmd::new(0x78, &[0x43], 0),
    Jd9365Cmd::new(0xE0, &[0x00], 0),
    Jd9365Cmd::new(0x11, &[0x00], 120),
    Jd9365Cmd::new(0x29, &[0x00], 20),
    Jd9365Cmd::new(0x35, &[0x00], 0),
];

/// Waveshare 3.4C carrier pin map. Values lifted from
/// `waveshare/esp32_p4_wifi6_touch_lcd_xc` BSP — confirmed correct
/// against the kit's schematic.
pub const WAVESHARE_3P4C_RESET_GPIO: i32 = 27;
pub const WAVESHARE_3P4C_BACKLIGHT_GPIO: i32 = 26;

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

/// MIPI-DSI bus + DPI timing for the Waveshare 3.4C. Values from BSP:
/// 2 lanes @ 1500 Mbps, 80 MHz DPI clock, 800x800 @ ~60 Hz with the
/// listed porches. RGB565 pixel format (matches firmware framebuffer).
#[derive(Debug, Clone, Copy)]
pub struct MipiDsiConfig {
    pub h_active: u32,
    pub v_active: u32,
    pub h_sync_pulse_width: u32,
    pub h_back_porch: u32,
    pub h_front_porch: u32,
    pub v_sync_pulse_width: u32,
    pub v_back_porch: u32,
    pub v_front_porch: u32,
    pub dpi_clock_mhz: u32,
    pub lane_bit_rate_mbps: u32,
    pub num_data_lanes: u8,
    pub init_cmds: &'static [Jd9365Cmd],
    pub reset_gpio: i32,
}

pub fn waveshare_3p4c_config() -> MipiDsiConfig {
    MipiDsiConfig {
        h_active: 800,
        v_active: 800,
        // BSP values (esp32_p4_wifi6_touch_lcd_xc.c, dpi_config block).
        h_sync_pulse_width: 20,
        h_back_porch: 20,
        h_front_porch: 40,
        v_sync_pulse_width: 4,
        v_back_porch: 12,
        v_front_porch: 24,
        dpi_clock_mhz: 80,
        lane_bit_rate_mbps: 1500,
        num_data_lanes: 2,
        init_cmds: WAVESHARE_3P4C_INIT_CMDS,
        reset_gpio: WAVESHARE_3P4C_RESET_GPIO,
    }
}

/// Concrete `EspIdfPanel` driving a JD9365 over MIPI-DSI on the
/// Waveshare 3.4C. Owns the DSI bus, the DBI command IO, and the
/// JD9365 driver instance. `present()` issues `esp_lcd_panel_draw_bitmap`
/// against the DPI panel.
#[derive(Debug)]
pub struct MipiDsiPanel {
    config: MipiDsiConfig,
    bus: esp_lcd_dsi_bus_handle_t,
    dbi_io: esp_lcd_panel_io_handle_t,
    panel: esp_lcd_panel_handle_t,
    initialized: bool,
    // Backing memory for the C `jd9365_lcd_init_cmd_t` array — the JD9365
    // driver expects a pointer that stays valid while the panel exists.
    _c_init_cmds: Vec<jd9365_lcd_init_cmd_t>,
    // The Vec inside `dpi_config` must outlive `panel`; we own it.
    _dpi_config: Box<esp_lcd_dpi_panel_config_t>,
}

impl MipiDsiPanel {
    /// Allocate the DSI bus, DBI command IO, and JD9365 panel.
    /// `esp_lcd_new_panel_jd9365` does not take a viewport size — it
    /// reads h/v from the embedded DPI config — so we build the full
    /// panel here and skip `EspIdfPanel::initialize`'s cross-check.
    pub fn new(config: MipiDsiConfig) -> Result<Self, EspIdfError> {
        unsafe {
            let bus_config = esp_lcd_dsi_bus_config_t {
                bus_id: 0,
                num_data_lanes: config.num_data_lanes,
                phy_clk_src: soc_periph_mipi_dsi_phy_clk_src_t_MIPI_DSI_PHY_CLK_SRC_DEFAULT,
                lane_bit_rate_mbps: config.lane_bit_rate_mbps,
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

            let dpi_config = Box::new(esp_lcd_dpi_panel_config_t {
                virtual_channel: 0,
                dpi_clk_src: soc_periph_mipi_dsi_dpi_clk_src_t_MIPI_DSI_DPI_CLK_SRC_DEFAULT,
                dpi_clock_freq_mhz: config.dpi_clock_mhz,
                pixel_format: lcd_color_rgb_pixel_format_t_LCD_COLOR_PIXEL_FORMAT_RGB565,
                in_color_format: lcd_color_format_t_LCD_COLOR_FMT_RGB565,
                out_color_format: lcd_color_format_t_LCD_COLOR_FMT_RGB565,
                num_fbs: 1,
                video_timing: esp_lcd_video_timing_t {
                    h_size: config.h_active,
                    v_size: config.v_active,
                    hsync_back_porch: config.h_back_porch,
                    hsync_pulse_width: config.h_sync_pulse_width,
                    hsync_front_porch: config.h_front_porch,
                    vsync_back_porch: config.v_back_porch,
                    vsync_pulse_width: config.v_sync_pulse_width,
                    vsync_front_porch: config.v_front_porch,
                },
                flags: esp_lcd_dpi_panel_config_t_extra_dpi_panel_flags::default(),
            });

            // Project the static Rust init array into the C struct shape
            // the JD9365 driver expects. Vec backs the C pointer; we keep
            // it alive by storing it in MipiDsiPanel.
            let c_init_cmds: Vec<jd9365_lcd_init_cmd_t> = config
                .init_cmds
                .iter()
                .map(|c| jd9365_lcd_init_cmd_t {
                    cmd: c.cmd as c_int,
                    data: c.params.as_ptr() as *const c_void,
                    data_bytes: c.params.len(),
                    delay_ms: c.delay_ms as c_uint,
                })
                .collect();

            let vendor_config = jd9365_vendor_config_t {
                init_cmds: c_init_cmds.as_ptr(),
                init_cmds_size: c_init_cmds.len() as u16,
                mipi_config: sys::jd9365_vendor_config_t__bindgen_ty_1 {
                    dsi_bus: bus,
                    dpi_config: &*dpi_config as *const _,
                    lane_num: config.num_data_lanes,
                },
            };

            let dev_config = esp_lcd_panel_dev_config_t {
                reset_gpio_num: config.reset_gpio,
                __bindgen_anon_1: sys::esp_lcd_panel_dev_config_t__bindgen_ty_1 {
                    rgb_ele_order: sys::lcd_rgb_element_order_t_LCD_RGB_ELEMENT_ORDER_RGB,
                },
                data_endian: sys::lcd_rgb_data_endian_t_LCD_RGB_DATA_ENDIAN_BIG,
                bits_per_pixel: 16,
                flags: Default::default(),
                vendor_config: &vendor_config as *const _ as *mut c_void,
            };

            let mut panel: esp_lcd_panel_handle_t = ptr::null_mut();
            check(esp_lcd_new_panel_jd9365(dbi_io, &dev_config, &mut panel))?;
            check(esp_lcd_panel_reset(panel))?;
            check(esp_lcd_panel_init(panel))?;

            Ok(Self {
                config,
                bus,
                dbi_io,
                panel,
                initialized: true,
                _c_init_cmds: c_init_cmds,
                _dpi_config: dpi_config,
            })
        }
    }
}

impl Drop for MipiDsiPanel {
    fn drop(&mut self) {
        unsafe {
            if !self.panel.is_null() {
                let _ = esp_lcd_panel_del(self.panel);
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
        // JD9365 was already brought up in `new`; just sanity-check
        // that the runtime viewport matches the panel's native res.
        if config.viewport_size.width_px != self.config.h_active
            || config.viewport_size.height_px != self.config.v_active
        {
            return Err(EspIdfError::Unsupported(format!(
                "mipi-dsi panel timing {}x{} does not match DisplayConfig viewport {}x{}",
                self.config.h_active,
                self.config.v_active,
                config.viewport_size.width_px,
                config.viewport_size.height_px,
            )));
        }
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
                self.panel,
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

/// Acquire the on-chip LDO channel that powers the MIPI-DSI PHY (channel
/// 3 @ 2500 mV per BSP `BSP_MIPI_DSI_PHY_PWR_LDO_CHAN`). Keep the handle
/// alive for the platform's lifetime — dropping it powers the rail down.
pub fn acquire_mipi_dsi_phy_power() -> Result<esp_ldo_channel_handle_t, EspIdfError> {
    acquire_ldo(3, 2500)
}

fn acquire_ldo(chan_id: i32, voltage_mv: i32) -> Result<esp_ldo_channel_handle_t, EspIdfError> {
    let cfg = esp_ldo_channel_config_t {
        chan_id,
        voltage_mv,
        flags: Default::default(),
    };
    let mut handle: esp_ldo_channel_handle_t = core::ptr::null_mut();
    let status = unsafe { esp_ldo_acquire_channel(&cfg, &mut handle) };
    if status != sys::ESP_OK as esp_err_t {
        return Err(EspIdfError::Io(format!(
            "esp_ldo_acquire_channel({}, {}mV) failed: {}",
            chan_id, voltage_mv, status
        )));
    }
    Ok(handle)
}

/// Drive the panel backlight GPIO high. The Waveshare 3.4C wires the
/// backlight enable straight to a P4 GPIO (active-high).
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

fn check(status: esp_err_t) -> Result<(), EspIdfError> {
    if status == sys::ESP_OK as esp_err_t {
        Ok(())
    } else {
        Err(EspIdfError::Io(format!("esp_lcd call failed: {}", status)))
    }
}

#[allow(dead_code)]
pub fn touch_defaults_matching_panel(display: &DisplayConfig) -> TouchControllerConfig {
    TouchControllerConfig {
        logical_width_px: display.viewport_size.width_px,
        logical_height_px: display.viewport_size.height_px,
        ..TouchControllerConfig::default()
    }
}
