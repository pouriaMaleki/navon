use std::f64::consts::{FRAC_PI_4, PI};
use std::time::Duration;

use crate::api::{GpsSample, WorldPoint};

const EARTH_RADIUS_M: f64 = 6_378_137.0;
const MAX_MERCATOR_LAT_DEG: f64 = 85.051_128_78;
const MIN_HEADING_DELTA_M: f64 = 1.0;

#[derive(Debug, Clone, Copy)]
pub(crate) struct MotionIngestConfig {
    pub riding_speed_threshold_mps: f32,
    pub stopped_speed_threshold_mps: f32,
    pub gps_loss_stop_timeout: Duration,
    pub min_heading_displacement_m: f64,
    pub heading_filter_alpha: f32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct MotionState {
    pub last_fix: Option<GpsSample>,
    pub rider_world: WorldPoint,
    pub speed_mps: f32,
    pub travel_heading_rad: Option<f32>,
    pub filtered_motion_vector_m: Option<(f64, f64)>,
    pub is_moving: bool,
    pub gps_gap_duration: Duration,
    pub stopped_duration: Duration,
}

impl Default for MotionState {
    fn default() -> Self {
        Self {
            last_fix: None,
            rider_world: WorldPoint::ORIGIN,
            speed_mps: 0.0,
            travel_heading_rad: None,
            filtered_motion_vector_m: None,
            is_moving: false,
            gps_gap_duration: Duration::ZERO,
            stopped_duration: Duration::ZERO,
        }
    }
}

impl MotionState {
    pub(crate) fn ingest(
        &mut self,
        gps: Option<GpsSample>,
        dt: Duration,
        config: MotionIngestConfig,
    ) {
        let was_moving = self.is_moving;
        let Some(sample) = gps else {
            self.gps_gap_duration += dt;
            if self.gps_gap_duration >= config.gps_loss_stop_timeout {
                self.speed_mps = 0.0;
                self.is_moving = false;
                self.advance_stopped_duration(dt, was_moving);
            }
            return;
        };

        self.gps_gap_duration = Duration::ZERO;
        let previous_world = self.last_fix.map(project_gps_to_world);
        let current_world = project_gps_to_world(sample);
        self.rider_world = current_world;
        self.speed_mps = sample.speed_mps.max(0.0);

        self.is_moving = if self.is_moving {
            self.speed_mps > config.stopped_speed_threshold_mps
        } else {
            self.speed_mps >= config.riding_speed_threshold_mps
        };

        if self.is_moving {
            self.stopped_duration = Duration::ZERO;
            self.travel_heading_rad = derive_heading(
                previous_world,
                current_world,
                sample,
                config.min_heading_displacement_m,
                config.heading_filter_alpha,
                self.filtered_motion_vector_m,
                self.travel_heading_rad,
            );
            self.filtered_motion_vector_m = update_filtered_motion_vector(
                previous_world,
                current_world,
                config.min_heading_displacement_m,
                config.heading_filter_alpha,
                self.filtered_motion_vector_m,
            )
            .or(self.filtered_motion_vector_m);
        } else {
            self.advance_stopped_duration(dt, was_moving);
        }

        self.last_fix = Some(sample);
    }

    fn advance_stopped_duration(&mut self, dt: Duration, was_moving: bool) {
        self.stopped_duration = if was_moving {
            Duration::ZERO
        } else {
            self.stopped_duration + dt
        };
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
    min_heading_displacement_m: f64,
    heading_filter_alpha: f32,
    filtered_motion_vector_m: Option<(f64, f64)>,
    previous_heading_rad: Option<f32>,
) -> Option<f32> {
    let filtered_motion = update_filtered_motion_vector(
        previous_world,
        current_world,
        min_heading_displacement_m,
        heading_filter_alpha,
        filtered_motion_vector_m,
    );

    filtered_motion
        .map(heading_from_vector)
        .or(previous_heading_rad)
        .or(sample.course_rad.map(normalize_bearing_rad))
}

fn heading_from_delta(
    previous: WorldPoint,
    current: WorldPoint,
    min_heading_displacement_m: f64,
) -> Option<(f64, f64)> {
    let dx = current.x_m - previous.x_m;
    let dy = current.y_m - previous.y_m;
    if dx.hypot(dy) < min_heading_displacement_m.max(MIN_HEADING_DELTA_M) {
        return None;
    }

    Some((dx, dy))
}

fn update_filtered_motion_vector(
    previous_world: Option<WorldPoint>,
    current_world: WorldPoint,
    min_heading_displacement_m: f64,
    heading_filter_alpha: f32,
    previous_filtered_motion_vector_m: Option<(f64, f64)>,
) -> Option<(f64, f64)> {
    let (next_dx, next_dy) =
        heading_from_delta(previous_world?, current_world, min_heading_displacement_m)?;
    let alpha = f64::from(heading_filter_alpha.clamp(0.0, 1.0));
    Some(match previous_filtered_motion_vector_m {
        Some((prev_dx, prev_dy)) => (
            prev_dx + ((next_dx - prev_dx) * alpha),
            prev_dy + ((next_dy - prev_dy) * alpha),
        ),
        None => (next_dx, next_dy),
    })
}

fn heading_from_vector((dx, dy): (f64, f64)) -> f32 {
    normalize_bearing_rad(dx.atan2(dy) as f32)
}

pub fn normalize_bearing_rad(angle_rad: f32) -> f32 {
    let tau = (2.0 * PI) as f32;
    let mut normalized = angle_rad % tau;
    if normalized < 0.0 {
        normalized += tau;
    }
    if (tau - normalized).abs() < 1e-6 {
        0.0
    } else {
        normalized
    }
}
