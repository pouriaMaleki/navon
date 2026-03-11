pub const TOUCH_PANEL_WIDTH: u16 = 800;
pub const TOUCH_PANEL_HEIGHT: u16 = 800;

pub const TOUCH_I2C_ADDR_PRIMARY: u8 = 0x5D;
pub const TOUCH_I2C_ADDR_SECONDARY: u8 = 0x14;

pub const TOUCH_SDA_GPIO: u8 = 7;
pub const TOUCH_SCL_GPIO: u8 = 8;

// These lines exist on the Waveshare display assembly, but the exact GPIO
// mapping still needs schematic confirmation before hard-wiring them in code.
pub const TOUCH_INT_GPIO: Option<u8> = None;
pub const TOUCH_RST_GPIO: Option<u8> = None;

pub const TOUCH_I2C_FREQUENCY_KHZ: u32 = 400;
pub const TOUCH_MAX_TRACKED_POINTS: usize = 2;

pub const TOUCH_TAP_MAX_MS: u32 = 260;
pub const TOUCH_TAP_MOVE_PX: f32 = 10.0;
pub const TOUCH_PAN_SENSITIVITY: f32 = 2.0;
pub const TOUCH_PAN_DEADZONE_PX: f32 = 1.5;
