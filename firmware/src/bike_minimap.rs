use crate::minimap::{
    CameraControllerInput, CameraControllerState, CameraView, SAMPLE_BOUNDS, WorldPoint,
    north_indicator_hit_test,
};

const TOUCH_PAN_SENSITIVITY: f32 = crate::board_config::TOUCH_PAN_SENSITIVITY;

#[derive(Clone, Copy, Debug)]
pub struct GeoFix {
    pub lat: f64,
    pub lon: f64,
    pub heading_rad: f32,
    pub speed_mps: f32,
}

#[derive(Clone, Copy, Debug)]
pub struct BikeMinimapState {
    pub player: WorldPoint,
    pub rider_heading_rad: f32,
    pub speed_mps: f32,
    controller: CameraControllerState,
    pending_pan_dx: f32,
    pending_pan_dy: f32,
    pending_zoom_scale: f32,
    pending_rotate_delta_rad: f32,
    origin_lat: f64,
    origin_lon: f64,
}

impl BikeMinimapState {
    pub fn new(origin_lat: f64, origin_lon: f64) -> Self {
        let player = WorldPoint { x: 500, y: 500 };
        let mut controller = CameraControllerState::new(1.0);
        controller.update(
            CameraControllerInput {
                player,
                rider_heading_rad: 0.0,
                speed_mps: 0.0,
                pan_dx: 0.0,
                pan_dy: 0.0,
                zoom_scale: 1.0,
                rotate_delta_rad: 0.0,
            },
            0.0,
        );
        Self {
            player,
            rider_heading_rad: 0.0,
            speed_mps: 0.0,
            controller,
            pending_pan_dx: 0.0,
            pending_pan_dy: 0.0,
            pending_zoom_scale: 1.0,
            pending_rotate_delta_rad: 0.0,
            origin_lat,
            origin_lon,
        }
    }

    pub fn apply_gps(&mut self, fix: GeoFix) {
        self.rider_heading_rad = fix.heading_rad;
        self.speed_mps = fix.speed_mps;
        self.player = project_geo_to_sample(fix.lat, fix.lon, self.origin_lat, self.origin_lon);
    }

    pub fn apply_pan_gesture(&mut self, dx_world: f32, dy_world: f32) {
        self.pending_pan_dx += dx_world;
        self.pending_pan_dy += dy_world;
    }

    pub fn apply_pan_pixels(&mut self, dx_pixels: f32, dy_pixels: f32, screen_width_pixels: f32) {
        let zoom = self.controller.output().zoom.max(0.1);
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
        self.pending_zoom_scale *= scale;
    }

    pub fn apply_rotate_gesture(&mut self, rotate_delta_rad: f32) {
        if rotate_delta_rad.is_finite() {
            self.pending_rotate_delta_rad += rotate_delta_rad;
        }
    }

    pub fn toggle_temporary_north_up(&mut self) {
        self.controller.toggle_temporary_north_up();
    }

    pub fn on_touch_tap_normalized(
        &mut self,
        nx: f32,
        ny: f32,
        screen_w: usize,
        screen_h: usize,
    ) -> bool {
        let x = (nx.clamp(0.0, 1.0) * screen_w as f32).round() as i32;
        let y = (ny.clamp(0.0, 1.0) * screen_h as f32).round() as i32;
        if north_indicator_hit_test(screen_w, screen_h, x, y) {
            self.controller.request_north_up();
            return true;
        }
        false
    }

    pub fn tick(&mut self, dt_ms: f32) {
        self.controller.update(
            CameraControllerInput {
                player: self.player,
                rider_heading_rad: self.rider_heading_rad,
                speed_mps: self.speed_mps,
                pan_dx: self.pending_pan_dx,
                pan_dy: self.pending_pan_dy,
                zoom_scale: self.pending_zoom_scale,
                rotate_delta_rad: self.pending_rotate_delta_rad,
            },
            dt_ms,
        );
        self.pending_pan_dx = 0.0;
        self.pending_pan_dy = 0.0;
        self.pending_zoom_scale = 1.0;
        self.pending_rotate_delta_rad = 0.0;
    }

    pub fn camera_view(&self) -> CameraView {
        let camera = self.controller.output();
        CameraView {
            center: WorldPoint {
                x: (camera.follow_player.x as f32 + camera.pan_x) as i16,
                y: (camera.follow_player.y as f32 + camera.pan_y) as i16,
            },
            player: self.player,
            heading_rad: camera.heading_rad,
            rider_heading_rad: camera.rider_heading_rad,
            zoom: camera.zoom,
            base_bounds: SAMPLE_BOUNDS,
            background: 18,
            player_anchor_x: 0.5,
            player_anchor_y: camera.player_anchor_y,
            riding_mode: camera.riding_mode,
            interaction_active: camera.interaction_active,
        }
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
