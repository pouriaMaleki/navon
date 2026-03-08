use esp32_screen_render_core::{
    FrameBuffer, MinimapView, WAVESHARE_ESP32_P4_3_4, WAVESHARE_ESP32_P4_4_0, render_device_style,
    render_sample_device_style, sample_player_for_tick,
};
use wasm_bindgen::prelude::*;

mod generated_map;

#[wasm_bindgen]
pub struct MinimapWasmEmulator {
    width: usize,
    height: usize,
    pixels: Vec<u8>,
    tick: u32,
}

#[wasm_bindgen]
impl MinimapWasmEmulator {
    #[wasm_bindgen(constructor)]
    pub fn new(profile: u32) -> Self {
        let spec = match profile {
            1 => WAVESHARE_ESP32_P4_4_0,
            _ => WAVESHARE_ESP32_P4_3_4,
        };

        let mut emu = Self {
            width: spec.width,
            height: spec.height,
            pixels: vec![0; spec.width * spec.height],
            tick: 0,
        };
        emu.render_current();
        emu
    }

    pub fn width(&self) -> usize {
        self.width
    }

    pub fn height(&self) -> usize {
        self.height
    }

    pub fn tick(&self) -> u32 {
        self.tick
    }

    pub fn reset(&mut self) {
        self.tick = 0;
        self.render_current();
    }

    pub fn step(&mut self) {
        self.tick = self.tick.wrapping_add(1);
        self.render_current();
    }

    pub fn pixels_ptr(&self) -> *const u8 {
        self.pixels.as_ptr()
    }

    pub fn pixels_len(&self) -> usize {
        self.pixels.len()
    }
}

impl MinimapWasmEmulator {
    fn render_current(&mut self) {
        let mut frame = FrameBuffer::new(self.width, self.height, &mut self.pixels);
        if generated_map::HAS_MAP && !generated_map::MAP_LINES.is_empty() {
            let player = generated_map::MAP_PLAYER;
            let view = MinimapView {
                bounds: generated_map::MAP_BOUNDS,
                background: 18,
                player,
            };
            render_device_style(&mut frame, generated_map::MAP_LINES, &view);
        } else {
            let player = sample_player_for_tick(self.tick);
            render_sample_device_style(&mut frame, player);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn profile_zero_uses_800_mode() {
        let emu = MinimapWasmEmulator::new(0);
        assert_eq!(emu.width(), 800);
        assert_eq!(emu.height(), 800);
        assert_eq!(emu.pixels_len(), 800 * 800);
    }

    #[test]
    fn profile_one_uses_720_mode() {
        let emu = MinimapWasmEmulator::new(1);
        assert_eq!(emu.width(), 720);
        assert_eq!(emu.height(), 720);
        assert_eq!(emu.pixels_len(), 720 * 720);
    }
}
