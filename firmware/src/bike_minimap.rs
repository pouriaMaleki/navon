use crate::minimap::{CameraView, Line, SAMPLE_BOUNDS, SAMPLE_LINES, WorldBounds, WorldPoint};
use runtime_core::{
    GestureEvent, GpsFixEvent, LayerClass, LodMask, MapQuerySpec, MapSource, Runtime,
    RuntimeConfig, RuntimeFrameOutput, RuntimeInputFrame, Viewport,
};

const TOUCH_PAN_SENSITIVITY: f32 = crate::board_config::TOUCH_PAN_SENSITIVITY;
const MAX_VISIBLE_LINES: usize = 128;

#[derive(Clone, Copy, Debug)]
pub struct GeoFix {
    pub lat: f64,
    pub lon: f64,
    pub heading_rad: f32,
    pub speed_mps: f32,
}

#[derive(Clone, Copy, Debug)]
struct SampleMapSource;

impl MapSource for SampleMapSource {
    fn bounds(&self) -> WorldBounds {
        SAMPLE_BOUNDS
    }

    fn query<const MAX_VISIBLE: usize>(
        &self,
        query: &MapQuerySpec,
        out: &mut heapless::Vec<Line, MAX_VISIBLE>,
    ) {
        out.clear();
        if !query.lod_mask.allows(LayerClass::Major) && !query.lod_mask.allows(LayerClass::Critical)
        {
            return;
        }

        for line in SAMPLE_LINES {
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
            if out.push(line).is_err() {
                break;
            }
        }
    }
}

#[derive(Debug)]
pub struct BikeMinimapState {
    pub player: WorldPoint,
    pub rider_heading_rad: f32,
    pub speed_mps: f32,
    runtime: Runtime<MAX_VISIBLE_LINES, SampleMapSource>,
    pending_input: RuntimeInputFrame,
    latest_gps: Option<GpsFixEvent>,
    last_output: RuntimeFrameOutput<MAX_VISIBLE_LINES>,
    origin_lat: f64,
    origin_lon: f64,
}

impl BikeMinimapState {
    pub fn new(origin_lat: f64, origin_lon: f64) -> Self {
        let mut runtime = Runtime::new(
            RuntimeConfig {
                viewport: Viewport {
                    width: crate::board_config::TOUCH_PANEL_WIDTH as usize,
                    height: crate::board_config::TOUCH_PANEL_HEIGHT as usize,
                },
                base_bounds: SAMPLE_BOUNDS,
                background: 18,
                initial_zoom: 1.0,
                player_anchor_x: 0.5,
            },
            SampleMapSource,
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

        Self {
            player: WorldPoint { x: 500, y: 500 },
            rider_heading_rad: 0.0,
            speed_mps: 0.0,
            runtime,
            pending_input: RuntimeInputFrame::default(),
            latest_gps: None,
            last_output,
            origin_lat,
            origin_lon,
        }
    }

    pub fn apply_gps(&mut self, fix: GeoFix) {
        self.rider_heading_rad = fix.heading_rad;
        self.speed_mps = fix.speed_mps;
        self.player = project_geo_to_sample(fix.lat, fix.lon, self.origin_lat, self.origin_lon);
        self.latest_gps = Some(GpsFixEvent {
            player: self.player,
            heading_rad: self.rider_heading_rad,
            speed_mps: self.speed_mps,
        });
    }

    pub fn apply_pan_gesture(&mut self, dx_world: f32, dy_world: f32) {
        let _ = self.pending_input.gestures.push(GestureEvent::Pan {
            dx: dx_world,
            dy: dy_world,
        });
    }

    pub fn apply_pan_pixels(&mut self, dx_pixels: f32, dy_pixels: f32, screen_width_pixels: f32) {
        let zoom = self.last_output.camera_view.zoom.max(0.1);
        let map_per_pixel =
            (10_000.0 / (screen_width_pixels.max(1.0) * zoom)) * TOUCH_PAN_SENSITIVITY;
        self.apply_pan_gesture(-dx_pixels * map_per_pixel, dy_pixels * map_per_pixel);
    }

    pub fn apply_pinch_gesture(&mut self, zoom_scale: f32) {
        let scale = if zoom_scale.is_finite() && zoom_scale > 0.0 {
            zoom_scale
        } else {
            1.0
        };
        let _ = self
            .pending_input
            .gestures
            .push(GestureEvent::Pinch { scale });
    }

    pub fn apply_rotate_gesture(&mut self, rotate_delta_rad: f32) {
        if rotate_delta_rad.is_finite() {
            let _ = self.pending_input.gestures.push(GestureEvent::Rotate {
                delta_rad: rotate_delta_rad,
            });
        }
    }

    pub fn toggle_temporary_north_up(&mut self) {
        self.pending_input.request_north_up = true;
    }

    pub fn on_touch_tap_normalized(
        &mut self,
        nx: f32,
        ny: f32,
        _screen_w: usize,
        _screen_h: usize,
    ) -> bool {
        self.pending_input.tap = Some(runtime_core::TapEvent {
            nx: nx.clamp(0.0, 1.0),
            ny: ny.clamp(0.0, 1.0),
        });
        true
    }

    pub fn tick(&mut self, dt_ms: f32) {
        self.pending_input.dt_ms = dt_ms;
        self.pending_input.gps_fix = self.latest_gps;
        let input = core::mem::take(&mut self.pending_input);
        self.last_output = self.runtime.step(input);
    }

    pub fn camera_view(&self) -> CameraView {
        self.last_output.camera_view
    }

    pub fn visible_lines(&self) -> &[Line] {
        self.last_output.visible_lines.as_slice()
    }
}

fn project_geo_to_sample(lat: f64, lon: f64, origin_lat: f64, origin_lon: f64) -> WorldPoint {
    // Small-area approximation for runtime scaffolding: map +/-500m around origin to SAMPLE_BOUNDS.
    let meters_per_deg_lat = 111_320.0;
    let meters_per_deg_lon = 111_320.0 * origin_lat.to_radians().cos().max(0.1);
    let dx_m = (lon - origin_lon) * meters_per_deg_lon;
    let dy_m = (lat - origin_lat) * meters_per_deg_lat;

    let nx = ((dx_m + 500.0) / 1000.0).clamp(0.0, 1.0);
    let ny = ((dy_m + 500.0) / 1000.0).clamp(0.0, 1.0);
    WorldPoint {
        x: (nx * 1000.0) as i16,
        y: (ny * 1000.0) as i16,
    }
}
