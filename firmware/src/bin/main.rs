#![no_std]
#![no_main]
#![deny(
    clippy::mem_forget,
    reason = "mem::forget is generally not safe to do with esp_hal types, especially those \
    holding buffers for the duration of a data transfer."
)]
#![deny(clippy::large_stack_frames)]

use esp_hal::clock::CpuClock;
use esp_hal::main;
use esp_hal::time::{Duration, Instant};
use esp_println::println;
use esp_backtrace as _;
use esp32_hello::minimap::{
    FrameBuffer, WAVESHARE_ESP32_P4_3_4, render_sample_device_style, sample_player_for_tick,
};

// Wokwi doesn't emulate the ESP32-P4 LCD target yet, so this is a compact
// viewport emulation of the 800x800 target render.
const MAP_W: usize = 160;
const MAP_H: usize = 160;
const ASCII_W: usize = 32;
const ASCII_H: usize = 16;

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
    let _peripherals = esp_hal::init(config);
    println!(
        "ESP32 minimap preview (target device {}x{})",
        WAVESHARE_ESP32_P4_3_4.width, WAVESHARE_ESP32_P4_3_4.height
    );

    let mut pixels = [0_u8; MAP_W * MAP_H];
    let mut t: u32 = 0;

    loop {
        let player = sample_player_for_tick(t);
        t = t.wrapping_add(1);

        let mut frame = FrameBuffer::new(MAP_W, MAP_H, &mut pixels);
        render_sample_device_style(&mut frame, player);

        println!("---- minimap frame ----");
        print_ascii_minimap(&pixels, MAP_W, MAP_H, ASCII_W, ASCII_H);

        let delay_start = Instant::now();
        while delay_start.elapsed() < Duration::from_millis(1200) {}
    }

    // for inspiration have a look at the examples at https://github.com/esp-rs/esp-hal/tree/esp-hal-v1.0.0/examples
}

fn print_ascii_minimap(
    pixels: &[u8],
    width: usize,
    height: usize,
    out_w: usize,
    out_h: usize,
) {
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
