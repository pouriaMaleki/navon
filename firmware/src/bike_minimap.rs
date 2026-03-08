use crate::minimap::{CameraView, SAMPLE_BOUNDS, WorldPoint};

#[derive(Clone, Copy, Debug)]
pub struct GeoFix {
    pub lat: f64,
    pub lon: f64,
    pub heading_rad: f32,
}

#[derive(Clone, Copy, Debug)]
pub struct BikeMinimapState {
    pub player: WorldPoint,
    pub heading_rad: f32,
    pub zoom: f32,
    pub pan_x: f32,
    pub pan_y: f32,
    idle_ms_since_pan: f32,
    origin_lat: f64,
    origin_lon: f64,
}

impl BikeMinimapState {
    pub fn new(origin_lat: f64, origin_lon: f64) -> Self {
        Self {
            player: WorldPoint { x: 500, y: 500 },
            heading_rad: 0.0,
            zoom: 1.0,
            pan_x: 0.0,
            pan_y: 0.0,
            idle_ms_since_pan: 0.0,
            origin_lat,
            origin_lon,
        }
    }

    pub fn apply_gps(&mut self, fix: GeoFix) {
        self.heading_rad = fix.heading_rad;
        self.player = project_geo_to_sample(fix.lat, fix.lon, self.origin_lat, self.origin_lon);
    }

    pub fn apply_pan_gesture(&mut self, dx_world: f32, dy_world: f32) {
        self.pan_x += dx_world;
        self.pan_y += dy_world;
        self.idle_ms_since_pan = 0.0;
    }

    pub fn apply_pinch_gesture(&mut self, zoom_scale: f32) {
        self.zoom = (self.zoom * zoom_scale).clamp(0.6, 4.5);
    }

    pub fn tick(&mut self, dt_ms: f32) {
        self.idle_ms_since_pan += dt_ms;
        if self.idle_ms_since_pan > 1200.0 {
            let t = (dt_ms / 220.0).clamp(0.0, 1.0);
            self.pan_x *= 1.0 - t;
            self.pan_y *= 1.0 - t;
        }
    }

    pub fn camera_view(&self) -> CameraView {
        CameraView {
            center: WorldPoint {
                x: (self.player.x as f32 + self.pan_x) as i16,
                y: (self.player.y as f32 + self.pan_y) as i16,
            },
            player: self.player,
            heading_rad: self.heading_rad,
            zoom: self.zoom,
            base_bounds: SAMPLE_BOUNDS,
            background: 18,
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
