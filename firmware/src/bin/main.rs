#![no_std]
#![no_main]
#![deny(
    clippy::mem_forget,
    reason = "mem::forget is generally not safe to do with esp_hal types, especially those \
    holding buffers for the duration of a data transfer."
)]
#![deny(clippy::large_stack_frames)]

use esp_backtrace as _;
use esp_hal::clock::CpuClock;
use esp_hal::i2c::master::{Config as I2cConfig, I2c};
use esp_hal::main;
use esp_hal::time::Rate;
use esp_hal::time::{Duration, Instant};
use esp_println::println;
use esp32_hello::bike_minimap::{BikeMinimapState, GeoFix};
use esp32_hello::board_config::{
    TOUCH_I2C_FREQUENCY_KHZ, TOUCH_PANEL_HEIGHT, TOUCH_PANEL_WIDTH, TOUCH_SCL_GPIO, TOUCH_SDA_GPIO,
};
use esp32_hello::minimap::{
    FrameBuffer, SAMPLE_LINES, WAVESHARE_ESP32_P4_3_4, render_device_style_camera,
};
use esp32_hello::touch::{TouchInput, TouchPanelConfig};

// Wokwi doesn't emulate the ESP32-P4 LCD target yet, so this is a compact
// viewport emulation of the 800x800 target render.
const MAP_W: usize = 160;
const MAP_H: usize = 160;
const ASCII_W: usize = 32;
const ASCII_H: usize = 16;
const FRAME_MS: u64 = 120;

// This creates a default app-descriptor required by the esp-idf bootloader.
// For more information see: <https://docs.espressif.com/projects/esp-idf/en/stable/esp32/api-reference/system/app_image_format.html#application-description>
esp_bootloader_esp_idf::esp_app_desc!();

#[allow(
    clippy::large_stack_frames,
    reason = "it's not unusual to allocate larger buffers etc. in main"
)]
#[main]
fn main() -> ! {
    // generator version: 1.2.0

    let config = esp_hal::Config::default().with_cpu_clock(CpuClock::max());
    let peripherals = esp_hal::init(config);
    println!(
        "ESP32 minimap preview (target device {}x{})",
        WAVESHARE_ESP32_P4_3_4.width, WAVESHARE_ESP32_P4_3_4.height
    );

    let i2c_config = I2cConfig::default().with_frequency(Rate::from_khz(TOUCH_I2C_FREQUENCY_KHZ));
    let touch_i2c = I2c::new(peripherals.I2C0, i2c_config)
        .expect("I2C0 init")
        .with_sda(peripherals.GPIO7)
        .with_scl(peripherals.GPIO8);
    let mut touch = match TouchInput::new_bus_only(
        touch_i2c,
        TouchPanelConfig {
            width: TOUCH_PANEL_WIDTH,
            height: TOUCH_PANEL_HEIGHT,
        },
    ) {
        Ok(input) => {
            println!(
                "touch input ready on I2C SDA GPIO{} / SCL GPIO{}",
                TOUCH_SDA_GPIO, TOUCH_SCL_GPIO
            );
            Some(input)
        }
        Err(_) => {
            println!("touch input unavailable; continuing without touch gestures");
            None
        }
    };

    let mut pixels = [0_u8; MAP_W * MAP_H];
    let mut t: u32 = 0;
    let mut bike = BikeMinimapState::new(60.17442, 24.94210);

    loop {
        let fix = mock_gps_fix(t);
        bike.apply_gps(fix);
        if let Some(touch) = touch.as_mut() {
            if let Ok(gesture) = touch.poll(t.saturating_mul(FRAME_MS as u32)) {
                bike.apply_pan_pixels(
                    gesture.pan_dx_px,
                    gesture.pan_dy_px,
                    TOUCH_PANEL_WIDTH as f32,
                );
                bike.apply_pinch_gesture(gesture.zoom_scale);
                bike.apply_rotate_gesture(gesture.rotate_delta_rad);
                if let Some(tap) = gesture.tap {
                    let _ = bike.on_touch_tap_normalized(
                        tap.x,
                        tap.y,
                        TOUCH_PANEL_WIDTH as usize,
                        TOUCH_PANEL_HEIGHT as usize,
                    );
                }
            }
        }
        bike.tick(FRAME_MS as f32);
        t = t.wrapping_add(1);

        let mut frame = FrameBuffer::new(MAP_W, MAP_H, &mut pixels);
        let view = bike.camera_view();
        render_device_style_camera(&mut frame, &SAMPLE_LINES, &view);

        println!("---- minimap frame ----");
        print_ascii_minimap(&pixels, MAP_W, MAP_H, ASCII_W, ASCII_H);

        let delay_start = Instant::now();
        while delay_start.elapsed() < Duration::from_millis(FRAME_MS) {}
    }

    // for inspiration have a look at the examples at https://github.com/esp-rs/esp-hal/tree/esp-hal-v1.0.0/examples
}

fn mock_gps_fix(tick: u32) -> GeoFix {
    let phase = tick % 180;
    let tt = tick as f32 * 0.06;
    let stop_t = 70.0 * 0.06;
    let stopped = (70..110).contains(&phase);
    let (lat, lon, heading_rad, speed_mps) = if stopped {
        (
            60.17442 + (stop_t.sin() as f64) * 0.0015,
            24.94210 + (stop_t.cos() as f64) * 0.0015,
            stop_t + core::f32::consts::FRAC_PI_2,
            0.0,
        )
    } else {
        (
            60.17442 + (tt.sin() as f64) * 0.0015,
            24.94210 + (tt.cos() as f64) * 0.0015,
            tt + core::f32::consts::FRAC_PI_2,
            3.8,
        )
    };
    GeoFix {
        lat,
        lon,
        heading_rad,
        speed_mps,
    }
}

fn print_ascii_minimap(pixels: &[u8], width: usize, height: usize, out_w: usize, out_h: usize) {
    let charset = [b' ', b'.', b':', b'-', b'=', b'+', b'*', b'#', b'@'];
    for row in 0..out_h {
        let mut line = [b' '; ASCII_W];
        let y0 = row * height / out_h;
        let y1 = ((row + 1) * height / out_h).max(y0 + 1);
        for (col, slot) in line.iter_mut().enumerate().take(out_w) {
            let x0 = col * width / out_w;
            let x1 = ((col + 1) * width / out_w).max(x0 + 1);
            let mut sum: u32 = 0;
            let mut n: u32 = 0;
            for y in y0..y1 {
                for x in x0..x1 {
                    sum += pixels[y * width + x] as u32;
                    n += 1;
                }
            }
            let avg = (sum / n.max(1)) as usize;
            let idx = (avg * (charset.len() - 1)) / 255;
            *slot = charset[idx];
        }
        if let Ok(s) = core::str::from_utf8(&line[..out_w]) {
            println!("{}", s);
        }
    }
}
