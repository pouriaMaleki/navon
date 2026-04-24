#[allow(unused_imports)]
use num_traits::Float as _;
use core::f64::consts::{FRAC_PI_4, PI};
use core::time::Duration;

use crate::api::{GpsSample, WorldPoint};

const EARTH_RADIUS_M: f64 = 6_378_137.0;
const MAX_MERCATOR_LAT_DEG: f64 = 85.051_128_78;
const MIN_HEADING_DELTA_M: f64 = 1.0;
const MIN_CONTINUED_MOTION_DELTA_M: f64 = 0.05;

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
    pub pending_motion_vector_m: Option<(f64, f64)>,
    pub heading_confident: bool,
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
            pending_motion_vector_m: None,
            heading_confident: false,
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
            self.advance_without_new_fix(dt, was_moving, config);
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
            let step_motion = previous_world.map(|previous| {
                let dx = current_world.x_m - previous.x_m;
                let dy = current_world.y_m - previous.y_m;
                (dx, dy)
            });
            let accumulated_motion = accumulate_motion_vector(
                step_motion,
                config.min_heading_displacement_m,
                self.pending_motion_vector_m,
            );
            self.pending_motion_vector_m = accumulated_motion.pending_motion_vector_m;
            let filtered_motion = update_filtered_motion_vector(
                accumulated_motion.accepted_motion_vector_m,
                previous_world,
                current_world,
                config.heading_filter_alpha,
                self.filtered_motion_vector_m,
            );
            let continuing_motion = step_motion
                .map(|(dx, dy)| dx.hypot(dy) >= MIN_CONTINUED_MOTION_DELTA_M)
                .unwrap_or(false);
            self.heading_confident = filtered_motion.is_some()
                || (self.filtered_motion_vector_m.is_some() && continuing_motion);
            if let Some(filtered_motion) = filtered_motion {
                self.filtered_motion_vector_m = Some(filtered_motion);
                self.travel_heading_rad = Some(heading_from_vector(filtered_motion));
            }
        } else {
            self.heading_confident = false;
            self.pending_motion_vector_m = None;
            self.advance_stopped_duration(dt, was_moving);
        }

        self.last_fix = Some(sample);
    }

    fn advance_without_new_fix(
        &mut self,
        dt: Duration,
        was_moving: bool,
        config: MotionIngestConfig,
    ) {
        self.gps_gap_duration += dt;
        if self.gps_gap_duration < config.gps_loss_stop_timeout {
            return;
        }

        self.speed_mps = 0.0;
        self.is_moving = false;
        self.heading_confident = false;
        self.pending_motion_vector_m = None;
        self.advance_stopped_duration(dt, was_moving);
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

#[derive(Debug, Clone, Copy, PartialEq)]
struct AccumulatedMotion {
    accepted_motion_vector_m: Option<(f64, f64)>,
    pending_motion_vector_m: Option<(f64, f64)>,
}

fn update_filtered_motion_vector(
    accepted_motion_vector_m: Option<(f64, f64)>,
    _previous_world: Option<WorldPoint>,
    _current_world: WorldPoint,
    heading_filter_alpha: f32,
    previous_filtered_motion_vector_m: Option<(f64, f64)>,
) -> Option<(f64, f64)> {
    let (next_dx, next_dy) = accepted_motion_vector_m?;
    let alpha = f64::from(heading_filter_alpha.clamp(0.0, 1.0));
    Some(match previous_filtered_motion_vector_m {
        Some((prev_dx, prev_dy)) => (
            prev_dx + ((next_dx - prev_dx) * alpha),
            prev_dy + ((next_dy - prev_dy) * alpha),
        ),
        None => (next_dx, next_dy),
    })
}

fn accumulate_motion_vector(
    step_motion_vector_m: Option<(f64, f64)>,
    min_heading_displacement_m: f64,
    pending_motion_vector_m: Option<(f64, f64)>,
) -> AccumulatedMotion {
    let Some((dx, dy)) = step_motion_vector_m else {
        return AccumulatedMotion {
            accepted_motion_vector_m: None,
            pending_motion_vector_m,
        };
    };

    let pending = pending_motion_vector_m.unwrap_or((0.0, 0.0));
    let accumulated = (pending.0 + dx, pending.1 + dy);
    if accumulated.0.hypot(accumulated.1) < min_heading_displacement_m.max(MIN_HEADING_DELTA_M) {
        return AccumulatedMotion {
            accepted_motion_vector_m: None,
            pending_motion_vector_m: Some(accumulated),
        };
    }

    AccumulatedMotion {
        accepted_motion_vector_m: Some(accumulated),
        pending_motion_vector_m: None,
    }
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
