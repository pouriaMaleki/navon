use crate::api::{
    CameraMode, CameraStateSnapshot, NormalizedScreenPoint, RuntimeConfig, ViewportSize, WorldPoint,
};
use crate::map::meters_per_pixel_for_zoom;
use crate::motion::MotionState;

#[derive(Debug, Clone, PartialEq)]
pub struct CameraState {
    pub mode: CameraMode,
    pub focus_world: WorldPoint,
    pub center_world: WorldPoint,
    pub zoom: f32,
    pub orientation_rad: f32,
    pub rider_anchor: NormalizedScreenPoint,
    pub follow_locked: bool,
    pub recenter_active: bool,
}

impl Default for CameraState {
    fn default() -> Self {
        Self {
            mode: CameraMode::Stopped,
            focus_world: WorldPoint::ORIGIN,
            center_world: WorldPoint::ORIGIN,
            zoom: RuntimeConfig::default().zoom_bounds.default,
            orientation_rad: 0.0,
            rider_anchor: NormalizedScreenPoint::CENTER,
            follow_locked: false,
            recenter_active: false,
        }
    }
}

impl CameraState {
    pub fn advance(
        &mut self,
        motion: &MotionState,
        viewport_size: ViewportSize,
        config: &RuntimeConfig,
    ) {
        self.mode = if motion.is_moving {
            CameraMode::Riding
        } else {
            CameraMode::Stopped
        };
        self.zoom = config.zoom_bounds.clamp(self.zoom);
        self.focus_world = motion.rider_world;
        self.orientation_rad = match self.mode {
            CameraMode::Riding => motion.travel_heading_rad.unwrap_or(self.orientation_rad),
            CameraMode::Stopped => 0.0,
        };
        self.rider_anchor = match self.mode {
            CameraMode::Riding => config.riding_rider_anchor,
            CameraMode::Stopped => config.stopped_rider_anchor,
        };
        self.center_world = center_world_for_focus(
            self.focus_world,
            self.orientation_rad,
            self.rider_anchor,
            viewport_size,
            self.zoom,
        );
        self.follow_locked = false;
        self.recenter_active = false;
    }

    pub fn snapshot(&self) -> CameraStateSnapshot {
        CameraStateSnapshot {
            mode: self.mode,
            focus_world: self.focus_world,
            center_world: self.center_world,
            zoom: self.zoom,
            orientation_rad: self.orientation_rad,
            rider_anchor: self.rider_anchor,
            follow_locked: self.follow_locked,
            recenter_active: self.recenter_active,
        }
    }
}

fn center_world_for_focus(
    focus_world: WorldPoint,
    orientation_rad: f32,
    rider_anchor: NormalizedScreenPoint,
    viewport_size: ViewportSize,
    zoom: f32,
) -> WorldPoint {
    if viewport_size.is_empty() {
        return focus_world;
    }

    let meters_per_pixel = meters_per_pixel_for_zoom(zoom);
    let screen_dx_px = (0.5 - f64::from(rider_anchor.x)) * f64::from(viewport_size.width_px);
    let screen_dy_px = (0.5 - f64::from(rider_anchor.y)) * f64::from(viewport_size.height_px);
    let local_east_m = screen_dx_px * meters_per_pixel;
    let local_north_m = -screen_dy_px * meters_per_pixel;
    let sin_theta = f64::from(orientation_rad).sin();
    let cos_theta = f64::from(orientation_rad).cos();
    let world_dx = (local_east_m * cos_theta) + (local_north_m * sin_theta);
    let world_dy = (-local_east_m * sin_theta) + (local_north_m * cos_theta);
    focus_world.translate(world_dx, world_dy)
}
