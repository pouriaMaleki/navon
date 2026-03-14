use std::f64::consts::{FRAC_PI_4, PI};
use std::time::Duration;

use crate::api::{GpsSample, WorldPoint};

const EARTH_RADIUS_M: f64 = 6_378_137.0;
const MAX_MERCATOR_LAT_DEG: f64 = 85.051_128_78;
const MIN_HEADING_DELTA_M: f64 = 1.0;

#[derive(Debug, Clone, PartialEq)]
pub struct MotionState {
    pub last_fix: Option<GpsSample>,
    pub rider_world: WorldPoint,
    pub speed_mps: f32,
    pub travel_heading_rad: Option<f32>,
    pub is_moving: bool,
    pub gps_gap_duration: Duration,
}

impl Default for MotionState {
    fn default() -> Self {
        Self {
            last_fix: None,
            rider_world: WorldPoint::ORIGIN,
            speed_mps: 0.0,
            travel_heading_rad: None,
            is_moving: false,
            gps_gap_duration: Duration::ZERO,
        }
    }
}

impl MotionState {
    pub fn ingest(
        &mut self,
        gps: Option<GpsSample>,
        dt: Duration,
        riding_speed_threshold_mps: f32,
        stopped_speed_threshold_mps: f32,
        gps_loss_stop_timeout: Duration,
    ) {
        let Some(sample) = gps else {
            self.gps_gap_duration += dt;
            if self.gps_gap_duration >= gps_loss_stop_timeout {
                self.speed_mps = 0.0;
                self.is_moving = false;
            }
            return;
        };

        self.gps_gap_duration = Duration::ZERO;
        let previous_world = self.last_fix.map(project_gps_to_world);
        let current_world = project_gps_to_world(sample);
        self.rider_world = current_world;
        self.speed_mps = sample.speed_mps.max(0.0);

        self.is_moving = if self.is_moving {
            self.speed_mps > stopped_speed_threshold_mps
        } else {
            self.speed_mps >= riding_speed_threshold_mps
        };

        if self.is_moving {
            self.travel_heading_rad =
                derive_heading(previous_world, current_world, sample).or(self.travel_heading_rad);
        }

        self.last_fix = Some(sample);
    }
}

pub fn project_gps_to_world(sample: GpsSample) -> WorldPoint {
    let lat_rad = sample
        .lat_deg
        .clamp(-MAX_MERCATOR_LAT_DEG, MAX_MERCATOR_LAT_DEG)
        .to_radians();
    let lon_rad = sample.lon_deg.to_radians();
    let x_m = EARTH_RADIUS_M * lon_rad;
    let y_m = EARTH_RADIUS_M * (FRAC_PI_4 + (lat_rad / 2.0)).tan().ln();
    WorldPoint::new(x_m, y_m)
}

fn derive_heading(
    previous_world: Option<WorldPoint>,
    current_world: WorldPoint,
    sample: GpsSample,
) -> Option<f32> {
    previous_world
        .and_then(|previous| heading_from_delta(previous, current_world))
        .or(sample.course_rad.map(normalize_bearing_rad))
}

fn heading_from_delta(previous: WorldPoint, current: WorldPoint) -> Option<f32> {
    let dx = current.x_m - previous.x_m;
    let dy = current.y_m - previous.y_m;
    if dx.hypot(dy) < MIN_HEADING_DELTA_M {
        return None;
    }

    Some(normalize_bearing_rad(dx.atan2(dy) as f32))
}

pub fn normalize_bearing_rad(angle_rad: f32) -> f32 {
    let tau = (2.0 * PI) as f32;
    let mut normalized = angle_rad % tau;
    if normalized < 0.0 {
        normalized += tau;
    }
    normalized
}
