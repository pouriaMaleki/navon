use esp32_screen_render_core::{
    CameraControllerInput, CameraControllerState, CameraView, FrameBuffer, SAMPLE_BOUNDS,
    SAMPLE_LINES, WAVESHARE_ESP32_P4_3_4, WAVESHARE_ESP32_P4_4_0, WorldPoint,
    north_indicator_hit_test, render_device_style_camera, sample_player_for_tick,
};
use wasm_bindgen::prelude::*;

mod generated_map;

const DEFAULT_INITIAL_ZOOM: f32 = 2.2;
const STATIONARY_RADIUS_M: f32 = 300.0;
const EARTH_RADIUS_M: f64 = 6_371_000.0;

#[wasm_bindgen]
pub struct MinimapWasmEmulator {
    width: usize,
    height: usize,
    pixels: Vec<u8>,
    tick: u32,
    has_geo: bool,
    geo_lat: f64,
    geo_lon: f64,
    geo_speed_mps: f32,
    rider_heading_rad: f32,
    pending_pan_dx: f32,
    pending_pan_dy: f32,
    pending_zoom_scale: f32,
    pending_rotate_rad: f32,
    controller: CameraControllerState,
    player: WorldPoint,
}

#[wasm_bindgen]
impl MinimapWasmEmulator {
    #[wasm_bindgen(constructor)]
    pub fn new(profile: u32) -> Self {
        let spec = match profile {
            1 => WAVESHARE_ESP32_P4_4_0,
            _ => WAVESHARE_ESP32_P4_3_4,
        };

        let initial_zoom = initial_zoom_for_stationary_radius();
        let mut emu = Self {
            width: spec.width,
            height: spec.height,
            pixels: vec![0; spec.width * spec.height],
            tick: 0,
            has_geo: false,
            geo_lat: 0.0,
            geo_lon: 0.0,
            geo_speed_mps: 0.0,
            rider_heading_rad: 0.0,
            pending_pan_dx: 0.0,
            pending_pan_dy: 0.0,
            pending_zoom_scale: 1.0,
            pending_rotate_rad: 0.0,
            controller: CameraControllerState::new(initial_zoom),
            player: sample_player_for_tick(0),
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
        self.pending_pan_dx = 0.0;
        self.pending_pan_dy = 0.0;
        self.pending_zoom_scale = 1.0;
        self.pending_rotate_rad = 0.0;
        self.controller = CameraControllerState::new(initial_zoom_for_stationary_radius());
        self.player = sample_player_for_tick(0);
        self.render_current();
    }

    pub fn step(&mut self, dt_ms: f32) {
        self.tick = self.tick.wrapping_add(1);
        self.player = if generated_map::HAS_MAP && !generated_map::MAP_LINES.is_empty() {
            if self.has_geo {
                geo_to_map_point(self.geo_lat, self.geo_lon)
            } else {
                generated_map::MAP_PLAYER
            }
        } else {
            sample_player_for_tick(self.tick)
        };

        let speed = if self.has_geo {
            self.geo_speed_mps
        } else {
            2.0
        };
        self.controller.update(
            CameraControllerInput {
                player: self.player,
                rider_heading_rad: self.rider_heading_rad,
                speed_mps: speed,
                pan_dx: self.pending_pan_dx,
                pan_dy: self.pending_pan_dy,
                zoom_scale: self.pending_zoom_scale,
                rotate_delta_rad: self.pending_rotate_rad,
            },
            dt_ms.max(0.0),
        );
        self.pending_pan_dx = 0.0;
        self.pending_pan_dy = 0.0;
        self.pending_zoom_scale = 1.0;
        self.pending_rotate_rad = 0.0;
        self.render_current();
    }

    pub fn pixels_ptr(&self) -> *const u8 {
        self.pixels.as_ptr()
    }

    pub fn pixels_len(&self) -> usize {
        self.pixels.len()
    }

    pub fn set_user_geo(&mut self, lat: f64, lon: f64, heading_rad: f32, speed_mps: f32) {
        self.has_geo = true;
        self.geo_lat = lat;
        self.geo_lon = lon;
        self.rider_heading_rad = heading_rad;
        self.geo_speed_mps = speed_mps.max(0.0);
    }

    pub fn set_gesture_deltas(
        &mut self,
        pan_dx: f32,
        pan_dy: f32,
        zoom_scale: f32,
        rotate_delta_rad: f32,
        interaction_active: bool,
    ) {
        self.pending_pan_dx += pan_dx;
        self.pending_pan_dy += pan_dy;
        if zoom_scale.is_finite() && zoom_scale > 0.0 {
            self.pending_zoom_scale *= zoom_scale;
        }
        if rotate_delta_rad.is_finite() {
            self.pending_rotate_rad += rotate_delta_rad;
        }
        if interaction_active {
            // Touch-active frames should be treated as recent interaction.
            self.controller.update(
                CameraControllerInput {
                    player: self.player,
                    rider_heading_rad: self.rider_heading_rad,
                    speed_mps: self.geo_speed_mps,
                    pan_dx: 0.0,
                    pan_dy: 0.0,
                    zoom_scale: 1.0,
                    rotate_delta_rad: 0.0,
                },
                0.0,
            );
        }
    }

    pub fn toggle_temporary_north_up(&mut self) {
        self.controller.toggle_temporary_north_up();
    }

    pub fn request_north_up(&mut self) {
        self.controller.request_north_up();
    }

    pub fn tap_normalized(&mut self, nx: f32, ny: f32) -> bool {
        let x = (nx.clamp(0.0, 1.0) * self.width as f32).round() as i32;
        let y = (ny.clamp(0.0, 1.0) * self.height as f32).round() as i32;
        if north_indicator_hit_test(self.width, self.height, x, y) {
            self.controller.request_north_up();
            return true;
        }
        false
    }

    pub fn camera_heading_rad(&self) -> f32 {
        self.controller.output().heading_rad
    }

    pub fn camera_zoom(&self) -> f32 {
        self.controller.output().zoom
    }

    pub fn camera_mode(&self) -> u8 {
        match self.controller.output().mode {
            esp32_screen_render_core::CameraMode::Riding => 0,
            esp32_screen_render_core::CameraMode::StoppedNorthUp => 1,
            esp32_screen_render_core::CameraMode::TemporaryNorthUp => 2,
        }
    }
}

impl MinimapWasmEmulator {
    fn render_current(&mut self) {
        let mut frame = FrameBuffer::new(self.width, self.height, &mut self.pixels);
        let camera = self.controller.output();
        let center = WorldPoint {
            x: (camera.follow_player.x as f32 + camera.pan_x) as i16,
            y: (camera.follow_player.y as f32 + camera.pan_y) as i16,
        };

        if generated_map::HAS_MAP && !generated_map::MAP_LINES.is_empty() {
            let view = CameraView {
                center,
                player: self.player,
                heading_rad: camera.heading_rad,
                rider_heading_rad: camera.rider_heading_rad,
                zoom: camera.zoom,
                base_bounds: generated_map::MAP_BOUNDS,
                background: 12,
                player_anchor_x: 0.5,
                player_anchor_y: camera.player_anchor_y,
                riding_mode: camera.riding_mode,
                interaction_active: camera.interaction_active,
            };
            render_device_style_camera(&mut frame, generated_map::MAP_LINES, &view);
        } else {
            let view = CameraView {
                center,
                player: self.player,
                heading_rad: camera.heading_rad,
                rider_heading_rad: camera.rider_heading_rad,
                zoom: camera.zoom,
                base_bounds: SAMPLE_BOUNDS,
                background: 12,
                player_anchor_x: 0.5,
                player_anchor_y: camera.player_anchor_y,
                riding_mode: camera.riding_mode,
                interaction_active: camera.interaction_active,
            };
            render_device_style_camera(&mut frame, &SAMPLE_LINES, &view);
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
    let ty = (1.0 - ((lat_rad.tan() + 1.0 / lat_rad.cos()).ln() / core::f64::consts::PI)) / 2.0 * n;
    let wx = (tx * 4096.0).round() as i32;
    let wy = -(ty * 4096.0).round() as i32;
    (wx, wy)
}

fn initial_zoom_for_stationary_radius() -> f32 {
    if !(generated_map::HAS_MAP && !generated_map::MAP_LINES.is_empty()) {
        return DEFAULT_INITIAL_ZOOM;
    }
    let world_span = (generated_map::MAP_WORLD_MAX_X - generated_map::MAP_WORLD_MIN_X).abs() as f64;
    if world_span <= 1.0 {
        return DEFAULT_INITIAL_ZOOM;
    }
    let lat_rad = generated_map::MAP_CENTER_LAT.to_radians();
    let world_units_per_globe = (1_u64 << generated_map::MAP_ZOOM) as f64 * 4096.0;
    let meters_per_world_unit =
        (2.0 * core::f64::consts::PI * EARTH_RADIUS_M * lat_rad.cos()) / world_units_per_globe;
    let meters_per_normalized = meters_per_world_unit * world_span / 10_000.0;
    let width_m_at_zoom_1 = 10_000.0 * meters_per_normalized;
    let target_width_m = (STATIONARY_RADIUS_M * 2.0) as f64;
    if target_width_m <= 0.0 {
        return DEFAULT_INITIAL_ZOOM;
    }
    (width_m_at_zoom_1 / target_width_m) as f32
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

    #[test]
    fn stationary_geo_returns_towards_north_up() {
        let mut emu = MinimapWasmEmulator::new(0);
        emu.set_user_geo(60.17442, 24.94210, 1.2, 2.4);
        for _ in 0..10 {
            emu.step(120.0);
        }
        emu.set_user_geo(60.17442, 24.94210, 1.2, 0.0);
        for _ in 0..20 {
            emu.step(120.0);
        }
        assert!(emu.camera_heading_rad().abs() < 0.3);
        assert_eq!(emu.camera_mode(), 1);
    }

    #[test]
    fn pan_lock_holds_follow_target_during_geo_motion() {
        let mut emu = MinimapWasmEmulator::new(0);
        let mut lon = 24.94210;
        emu.set_user_geo(60.17442, lon, 0.0, 3.0);
        emu.step(120.0);

        emu.set_gesture_deltas(35.0, -12.0, 1.0, 0.0, true);
        emu.step(120.0);
        let locked = emu.controller.output().follow_player;

        for _ in 0..6 {
            lon += 0.001;
            emu.set_user_geo(60.17442, lon, 0.0, 3.0);
            emu.step(120.0);
            assert_eq!(emu.controller.output().follow_player, locked);
        }
        assert_ne!(emu.player, locked);

        for _ in 0..30 {
            lon += 0.001;
            emu.set_user_geo(60.17442, lon, 0.0, 3.0);
            emu.step(120.0);
        }
        let out = emu.controller.output();
        assert_eq!(out.follow_player, emu.player);
    }
}
