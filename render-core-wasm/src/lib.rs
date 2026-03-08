use esp32_screen_render_core::{
    CameraView, FrameBuffer, WorldPoint, WAVESHARE_ESP32_P4_3_4, WAVESHARE_ESP32_P4_4_0,
    render_device_style_camera, render_sample_device_style, sample_player_for_tick,
};
use wasm_bindgen::prelude::*;

mod generated_map;

#[wasm_bindgen]
pub struct MinimapWasmEmulator {
    width: usize,
    height: usize,
    pixels: Vec<u8>,
    tick: u32,
    has_geo: bool,
    geo_lat: f64,
    geo_lon: f64,
    heading_rad: f32,
    zoom: f32,
    pan_x: f32,
    pan_y: f32,
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
            has_geo: false,
            geo_lat: 0.0,
            geo_lon: 0.0,
            heading_rad: 0.0,
            zoom: 2.2,
            pan_x: 0.0,
            pan_y: 0.0,
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
        self.zoom = 2.2;
        self.pan_x = 0.0;
        self.pan_y = 0.0;
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

    pub fn set_user_geo(&mut self, lat: f64, lon: f64, heading_rad: f32) {
        self.has_geo = true;
        self.geo_lat = lat;
        self.geo_lon = lon;
        self.heading_rad = heading_rad;
    }

    pub fn set_camera(&mut self, zoom: f32, pan_x: f32, pan_y: f32) {
        self.zoom = zoom.clamp(0.5, 5.0);
        self.pan_x = pan_x;
        self.pan_y = pan_y;
    }
}

impl MinimapWasmEmulator {
    fn render_current(&mut self) {
        let mut frame = FrameBuffer::new(self.width, self.height, &mut self.pixels);
        if generated_map::HAS_MAP && !generated_map::MAP_LINES.is_empty() {
            let mut player = generated_map::MAP_PLAYER;
            if self.has_geo {
                player = geo_to_map_point(self.geo_lat, self.geo_lon);
            }
            let center = WorldPoint {
                x: (player.x as f32 + self.pan_x) as i16,
                y: (player.y as f32 + self.pan_y) as i16,
            };
            let view = CameraView {
                center,
                player,
                heading_rad: self.heading_rad,
                zoom: self.zoom,
                base_bounds: generated_map::MAP_BOUNDS,
                background: 18,
            };
            render_device_style_camera(&mut frame, generated_map::MAP_LINES, &view);
        } else {
            let player = sample_player_for_tick(self.tick);
            render_sample_device_style(&mut frame, player);
        }
    }
}

fn geo_to_map_point(lat: f64, lon: f64) -> WorldPoint {
    let world = lonlat_to_world(lon, lat, generated_map::MAP_ZOOM);
    let min_x = generated_map::MAP_WORLD_MIN_X as f64;
    let max_x = generated_map::MAP_WORLD_MAX_X as f64;
    let min_y = generated_map::MAP_WORLD_MIN_Y as f64;
    let max_y = generated_map::MAP_WORLD_MAX_Y as f64;
    let nx = normalize(world.0 as f64, min_x, max_x);
    let ny = normalize(world.1 as f64, min_y, max_y);
    WorldPoint {
        x: nx as i16,
        y: ny as i16,
    }
}

fn normalize(v: f64, min_v: f64, max_v: f64) -> i32 {
    let range = (max_v - min_v).max(1.0);
    let pos = ((v - min_v) / range).clamp(0.0, 1.0);
    (pos * 10_000.0).round() as i32
}

fn lonlat_to_world(lon: f64, lat: f64, z: i32) -> (i32, i32) {
    let n = 2_f64.powi(z);
    let tx = (lon + 180.0) / 360.0 * n;
    let lat_rad = lat.to_radians();
    let ty =
        (1.0 - ((lat_rad.tan() + 1.0 / lat_rad.cos()).ln() / core::f64::consts::PI)) / 2.0 * n;
    let wx = (tx * 4096.0).round() as i32;
    let wy = -(ty * 4096.0).round() as i32;
    (wx, wy)
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
