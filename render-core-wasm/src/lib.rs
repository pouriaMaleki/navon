use esp32_screen_render_core::{
    FrameBuffer, Line, SAMPLE_BOUNDS, SAMPLE_LINES, WAVESHARE_ESP32_P4_3_4, WAVESHARE_ESP32_P4_4_0,
    WorldPoint, north_indicator_hit_test, render_device_style_camera, sample_player_for_tick,
};
use runtime_core::{
    GestureEvent, GpsFixEvent, LayerClass, LodMask, MapQuerySpec, MapSource, Runtime,
    RuntimeConfig, RuntimeFrameOutput, RuntimeInputFrame, Viewport,
};
use wasm_bindgen::prelude::*;

mod generated_map;

const DEFAULT_INITIAL_ZOOM: f32 = 2.2;
const STATIONARY_RADIUS_M: f32 = 300.0;
const EARTH_RADIUS_M: f64 = 6_371_000.0;
const MAX_VISIBLE_LINES: usize = 8192;

#[derive(Clone, Copy)]
enum WasmMapSource {
    Generated,
    Sample,
}

impl MapSource for WasmMapSource {
    fn bounds(&self) -> esp32_screen_render_core::WorldBounds {
        match self {
            Self::Generated => generated_map::MAP_BOUNDS,
            Self::Sample => SAMPLE_BOUNDS,
        }
    }

    fn query<const MAX_VISIBLE: usize>(
        &self,
        query: &MapQuerySpec,
        out: &mut heapless::Vec<Line, MAX_VISIBLE>,
    ) {
        out.clear();
        let lines: &[Line] = match self {
            Self::Generated => generated_map::MAP_LINES,
            Self::Sample => &SAMPLE_LINES,
        };
        for line in lines {
            if !query.lod_mask.allows(class_for_line(*line)) {
                continue;
            }
            let min_x = line.from.x.min(line.to.x);
            let max_x = line.from.x.max(line.to.x);
            let min_y = line.from.y.min(line.to.y);
            let max_y = line.from.y.max(line.to.y);
            if max_x < query.bounds.min_x
                || min_x > query.bounds.max_x
                || max_y < query.bounds.min_y
                || min_y > query.bounds.max_y
            {
                continue;
            }
            if out.push(*line).is_err() {
                break;
            }
        }
    }
}

fn class_for_line(line: Line) -> LayerClass {
    if line.thickness >= 3 {
        LayerClass::Critical
    } else if line.thickness >= 2 {
        LayerClass::Major
    } else if line.intensity >= 220 {
        LayerClass::Minor
    } else if line.intensity >= 150 {
        LayerClass::Local
    } else {
        LayerClass::Detail
    }
}

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
    pending_input: RuntimeInputFrame,
    runtime: Runtime<MAX_VISIBLE_LINES, WasmMapSource>,
    last_output: RuntimeFrameOutput<MAX_VISIBLE_LINES>,
    last_player: WorldPoint,
}

#[wasm_bindgen]
impl MinimapWasmEmulator {
    #[wasm_bindgen(constructor)]
    pub fn new(profile: u32) -> Self {
        let spec = match profile {
            1 => WAVESHARE_ESP32_P4_4_0,
            _ => WAVESHARE_ESP32_P4_3_4,
        };

        let source = if generated_map::HAS_MAP && !generated_map::MAP_LINES.is_empty() {
            WasmMapSource::Generated
        } else {
            WasmMapSource::Sample
        };
        let initial_zoom = initial_zoom_for_stationary_radius();
        let mut runtime = Runtime::new(
            RuntimeConfig {
                viewport: Viewport {
                    width: spec.width,
                    height: spec.height,
                },
                base_bounds: source.bounds(),
                background: 12,
                initial_zoom,
                player_anchor_x: 0.5,
            },
            source,
        );
        runtime.set_lod_policy(runtime_core::LodPolicy {
            zoom_thresholds: [1.0, 2.0, 4.0, 8.0],
            masks: [
                LodMask::CORE_ONLY,
                LodMask::CORE_ONLY,
                LodMask::MAJOR_AND_UP,
                LodMask::ALL,
                LodMask::ALL,
            ],
        });
        let last_output = runtime.step(RuntimeInputFrame::default());

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
            pending_input: RuntimeInputFrame::default(),
            runtime,
            last_output,
            last_player: sample_player_for_tick(0),
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
        self.pending_input = RuntimeInputFrame::default();
        self.runtime.reset();
        self.last_output = self.runtime.step(RuntimeInputFrame::default());
        self.last_player = sample_player_for_tick(0);
        self.render_current();
    }

    pub fn step(&mut self, dt_ms: f32) {
        self.tick = self.tick.wrapping_add(1);
        self.last_player = if generated_map::HAS_MAP && !generated_map::MAP_LINES.is_empty() {
            if self.has_geo {
                geo_to_map_point(self.geo_lat, self.geo_lon)
            } else {
                generated_map::MAP_PLAYER
            }
        } else {
            sample_player_for_tick(self.tick)
        };

        self.pending_input.dt_ms = dt_ms.max(0.0);
        self.pending_input.gps_fix = Some(GpsFixEvent {
            player: self.last_player,
            heading_rad: self.rider_heading_rad,
            speed_mps: if self.has_geo {
                self.geo_speed_mps
            } else {
                2.0
            },
        });

        let input = core::mem::take(&mut self.pending_input);
        self.last_output = self.runtime.step(input);
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
        _interaction_active: bool,
    ) {
        if pan_dx != 0.0 || pan_dy != 0.0 {
            let _ = self.pending_input.gestures.push(GestureEvent::Pan {
                dx: pan_dx,
                dy: pan_dy,
            });
        }
        if zoom_scale.is_finite() && zoom_scale > 0.0 && (zoom_scale - 1.0).abs() > f32::EPSILON {
            let _ = self
                .pending_input
                .gestures
                .push(GestureEvent::Pinch { scale: zoom_scale });
        }
        if rotate_delta_rad.is_finite() && rotate_delta_rad != 0.0 {
            let _ = self.pending_input.gestures.push(GestureEvent::Rotate {
                delta_rad: rotate_delta_rad,
            });
        }
    }

    pub fn toggle_temporary_north_up(&mut self) {
        self.pending_input.request_north_up = true;
    }

    pub fn request_north_up(&mut self) {
        self.pending_input.request_north_up = true;
    }

    pub fn tap_normalized(&mut self, nx: f32, ny: f32) -> bool {
        let x = (nx.clamp(0.0, 1.0) * self.width as f32).round() as i32;
        let y = (ny.clamp(0.0, 1.0) * self.height as f32).round() as i32;
        if north_indicator_hit_test(self.width, self.height, x, y) {
            self.pending_input.tap = Some(runtime_core::TapEvent {
                nx: nx.clamp(0.0, 1.0),
                ny: ny.clamp(0.0, 1.0),
            });
            return true;
        }
        false
    }

    pub fn camera_heading_rad(&self) -> f32 {
        self.last_output.camera_view.heading_rad
    }

    pub fn camera_zoom(&self) -> f32 {
        self.last_output.camera_view.zoom
    }

    pub fn camera_mode(&self) -> u8 {
        match self.last_output.camera_mode {
            esp32_screen_render_core::CameraMode::Riding => 0,
            esp32_screen_render_core::CameraMode::StoppedNorthUp => 1,
            esp32_screen_render_core::CameraMode::TemporaryNorthUp => 2,
        }
    }
}

impl MinimapWasmEmulator {
    fn render_current(&mut self) {
        let mut frame = FrameBuffer::new(self.width, self.height, &mut self.pixels);
        render_device_style_camera(
            &mut frame,
            self.last_output.visible_lines.as_slice(),
            &self.last_output.camera_view,
        );
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
    use runtime_core::{RuntimeConfig, RuntimeInputFrame, Viewport};

    fn approx(a: f32, b: f32, eps: f32) -> bool {
        (a - b).abs() <= eps
    }

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
        let locked_center = emu.last_output.camera_view.center;

        for _ in 0..6 {
            lon += 0.001;
            emu.set_user_geo(60.17442, lon, 0.0, 3.0);
            emu.step(120.0);
            assert!(
                (emu.last_output.camera_view.center.x as i32 - locked_center.x as i32).abs() < 8
            );
            assert!(
                (emu.last_output.camera_view.center.y as i32 - locked_center.y as i32).abs() < 8
            );
        }
    }

    #[test]
    fn wasm_matches_native_runtime_for_same_trace() {
        let source = if generated_map::HAS_MAP && !generated_map::MAP_LINES.is_empty() {
            WasmMapSource::Generated
        } else {
            WasmMapSource::Sample
        };
        let mut native = Runtime::<MAX_VISIBLE_LINES, WasmMapSource>::new(
            RuntimeConfig {
                viewport: Viewport {
                    width: WAVESHARE_ESP32_P4_3_4.width,
                    height: WAVESHARE_ESP32_P4_3_4.height,
                },
                base_bounds: source.bounds(),
                background: 12,
                initial_zoom: initial_zoom_for_stationary_radius(),
                player_anchor_x: 0.5,
            },
            source,
        );
        native.set_lod_policy(runtime_core::LodPolicy {
            zoom_thresholds: [1.0, 2.0, 4.0, 8.0],
            masks: [
                LodMask::CORE_ONLY,
                LodMask::CORE_ONLY,
                LodMask::MAJOR_AND_UP,
                LodMask::ALL,
                LodMask::ALL,
            ],
        });
        let mut emu = MinimapWasmEmulator::new(0);

        let mut lon = 24.94210;
        let mut lat = 60.17442;
        for frame in 0..28 {
            let moving = frame < 8 || frame >= 18;
            if moving {
                lon += 0.00035;
                lat += 0.00006;
            }
            let speed = if moving { 3.2 } else { 0.0 };
            let heading = if moving { 0.65 } else { 0.2 };
            let pan_dx = if (10..15).contains(&frame) { 24.0 } else { 0.0 };
            let pan_dy = if (10..15).contains(&frame) { -8.0 } else { 0.0 };
            let zoom = if frame == 12 { 1.18 } else { 1.0 };
            let rotate = if frame == 12 { 0.22 } else { 0.0 };
            let north_up_request = frame == 16;

            let player = if generated_map::HAS_MAP && !generated_map::MAP_LINES.is_empty() {
                geo_to_map_point(lat, lon)
            } else {
                sample_player_for_tick((frame + 1) as u32)
            };

            let mut input = RuntimeInputFrame {
                dt_ms: 120.0,
                gps_fix: Some(GpsFixEvent {
                    player,
                    heading_rad: heading,
                    speed_mps: speed,
                }),
                request_north_up: north_up_request,
                ..RuntimeInputFrame::default()
            };
            if pan_dx != 0.0 || pan_dy != 0.0 {
                let _ = input.gestures.push(GestureEvent::Pan {
                    dx: pan_dx,
                    dy: pan_dy,
                });
            }
            if zoom != 1.0 {
                let _ = input.gestures.push(GestureEvent::Pinch { scale: zoom });
            }
            if rotate != 0.0 {
                let _ = input
                    .gestures
                    .push(GestureEvent::Rotate { delta_rad: rotate });
            }
            let native_out = native.step(input);

            emu.set_user_geo(lat, lon, heading, speed);
            emu.set_gesture_deltas(pan_dx, pan_dy, zoom, rotate, pan_dx != 0.0 || pan_dy != 0.0);
            if north_up_request {
                emu.request_north_up();
            }
            emu.step(120.0);

            assert_eq!(emu.last_output.camera_mode, native_out.camera_mode);
            assert!(approx(
                emu.last_output.camera_view.heading_rad,
                native_out.camera_view.heading_rad,
                0.0005
            ));
            assert!(approx(
                emu.last_output.camera_view.zoom,
                native_out.camera_view.zoom,
                0.0005
            ));
            assert!(approx(
                emu.last_output.camera_view.player_anchor_y,
                native_out.camera_view.player_anchor_y,
                0.0005
            ));
            assert_eq!(
                emu.last_output.camera_view.center,
                native_out.camera_view.center
            );
            assert_eq!(
                emu.last_output.visible_lines.len(),
                native_out.visible_lines.len()
            );
        }
    }
}
