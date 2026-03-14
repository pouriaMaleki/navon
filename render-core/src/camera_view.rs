use runtime_core::api::{CameraStateSnapshot, ScreenPoint, ViewportSize, WorldPoint};

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CameraView {
    viewport: ViewportSize,
    center_world: WorldPoint,
    orientation_rad: f32,
    meters_per_pixel: f64,
}

impl CameraView {
    pub fn new(
        viewport: ViewportSize,
        camera: &CameraStateSnapshot,
        meters_per_pixel: f64,
    ) -> Self {
        Self {
            viewport,
            center_world: camera.center_world,
            orientation_rad: camera.orientation_rad,
            meters_per_pixel,
        }
    }

    pub fn world_to_screen(self, point: WorldPoint) -> ScreenPoint {
        let dx_world = point.x_m - self.center_world.x_m;
        let dy_world = point.y_m - self.center_world.y_m;
        let sin_theta = f64::from(self.orientation_rad).sin();
        let cos_theta = f64::from(self.orientation_rad).cos();
        let local_east_m = (dx_world * cos_theta) - (dy_world * sin_theta);
        let local_north_m = (dx_world * sin_theta) + (dy_world * cos_theta);
        let x_px =
            (f64::from(self.viewport.width_px) / 2.0) + (local_east_m / self.meters_per_pixel);
        let y_px =
            (f64::from(self.viewport.height_px) / 2.0) - (local_north_m / self.meters_per_pixel);
        ScreenPoint::new(x_px as f32, y_px as f32)
    }
}

impl Default for CameraView {
    fn default() -> Self {
        Self {
            viewport: ViewportSize::default(),
            center_world: WorldPoint::ORIGIN,
            orientation_rad: 0.0,
            meters_per_pixel: 1.0,
        }
    }
}

#[cfg(test)]
mod tests {
    use runtime_core::api::{CameraMode, NormalizedScreenPoint};

    use super::*;

    #[test]
    fn maps_center_world_to_screen_center() {
        let view = CameraView::new(
            ViewportSize::new(200, 100),
            &CameraStateSnapshot {
                mode: CameraMode::Stopped,
                focus_world: WorldPoint::ORIGIN,
                center_world: WorldPoint::new(25.0, 80.0),
                zoom: 15.5,
                orientation_rad: 0.0,
                rider_anchor: NormalizedScreenPoint::CENTER,
                follow_locked: false,
                recenter_active: false,
            },
            1.0,
        );

        let screen = view.world_to_screen(WorldPoint::new(25.0, 80.0));

        assert!((screen.x_px - 100.0).abs() < 0.01);
        assert!((screen.y_px - 50.0).abs() < 0.01);
    }
}
