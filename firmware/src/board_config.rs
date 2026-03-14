use std::time::Duration;

use runtime_core::api::ViewportSize;

pub const DISPLAY_WIDTH_PX: u32 = 800;
pub const DISPLAY_HEIGHT_PX: u32 = 800;
pub const TOUCH_I2C_SCL_GPIO: u8 = 8;
pub const TOUCH_I2C_SDA_GPIO: u8 = 7;
pub const TOUCH_MAX_CONTACTS: u8 = 10;
pub const TOUCH_CONTROLLER_MAX_X: u16 = 799;
pub const TOUCH_CONTROLLER_MAX_Y: u16 = 799;
pub const TOUCH_I2C_ADDRESS: u16 = 0x5d;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PanelPixelFormat {
    Rgb565Le,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TouchControllerConfig {
    pub i2c_scl_gpio: u8,
    pub i2c_sda_gpio: u8,
    pub int_gpio: Option<u8>,
    pub rst_gpio: Option<u8>,
    pub address: u16,
    pub max_contacts: u8,
    pub controller_max_x: u16,
    pub controller_max_y: u16,
    pub logical_width_px: u32,
    pub logical_height_px: u32,
    pub polling_interval: Duration,
}

impl Default for TouchControllerConfig {
    fn default() -> Self {
        Self {
            i2c_scl_gpio: TOUCH_I2C_SCL_GPIO,
            i2c_sda_gpio: TOUCH_I2C_SDA_GPIO,
            int_gpio: None,
            rst_gpio: None,
            address: TOUCH_I2C_ADDRESS,
            max_contacts: TOUCH_MAX_CONTACTS,
            controller_max_x: TOUCH_CONTROLLER_MAX_X,
            controller_max_y: TOUCH_CONTROLLER_MAX_Y,
            logical_width_px: DISPLAY_WIDTH_PX,
            logical_height_px: DISPLAY_HEIGHT_PX,
            polling_interval: Duration::from_millis(16),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DisplayConfig {
    pub viewport_size: ViewportSize,
    pub pixel_format: PanelPixelFormat,
    pub reset_gpio: Option<u8>,
    pub backlight_gpio: Option<u8>,
}

impl Default for DisplayConfig {
    fn default() -> Self {
        Self {
            viewport_size: ViewportSize::new(DISPLAY_WIDTH_PX, DISPLAY_HEIGHT_PX),
            pixel_format: PanelPixelFormat::Rgb565Le,
            reset_gpio: None,
            backlight_gpio: None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BoardConfig {
    pub viewport_size: ViewportSize,
    pub frame_interval: Duration,
    pub touch: TouchControllerConfig,
    pub display: DisplayConfig,
}

impl BoardConfig {
    pub const fn new(viewport_size: ViewportSize) -> Self {
        Self {
            viewport_size,
            frame_interval: Duration::from_millis(16),
            touch: TouchControllerConfig {
                i2c_scl_gpio: TOUCH_I2C_SCL_GPIO,
                i2c_sda_gpio: TOUCH_I2C_SDA_GPIO,
                int_gpio: None,
                rst_gpio: None,
                address: TOUCH_I2C_ADDRESS,
                max_contacts: TOUCH_MAX_CONTACTS,
                controller_max_x: TOUCH_CONTROLLER_MAX_X,
                controller_max_y: TOUCH_CONTROLLER_MAX_Y,
                logical_width_px: viewport_size.width_px,
                logical_height_px: viewport_size.height_px,
                polling_interval: Duration::from_millis(16),
            },
            display: DisplayConfig {
                viewport_size,
                pixel_format: PanelPixelFormat::Rgb565Le,
                reset_gpio: None,
                backlight_gpio: None,
            },
        }
    }

    pub const fn waveshare_esp32_p4_touch_lcd_3_4c() -> Self {
        Self::new(ViewportSize::new(DISPLAY_WIDTH_PX, DISPLAY_HEIGHT_PX))
    }
}

impl Default for BoardConfig {
    fn default() -> Self {
        Self::waveshare_esp32_p4_touch_lcd_3_4c()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn waveshare_board_config_uses_expected_display_and_touch_defaults() {
        let board = BoardConfig::default();

        assert_eq!(board.viewport_size, ViewportSize::new(800, 800));
        assert_eq!(board.touch.i2c_scl_gpio, 8);
        assert_eq!(board.touch.i2c_sda_gpio, 7);
        assert_eq!(board.touch.max_contacts, 10);
        assert_eq!(board.touch.logical_width_px, 800);
        assert_eq!(board.touch.logical_height_px, 800);
        assert_eq!(board.display.pixel_format, PanelPixelFormat::Rgb565Le);
    }
}
