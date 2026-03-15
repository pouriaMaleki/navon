mod assets;

use runtime_core::api::{CameraStateSnapshot, OverlayState, RuntimeConfig, ViewportSize};

use crate::camera_view::CameraView;
use crate::raster::Framebuffer;
use crate::style::RenderStyle;

pub fn draw_overlay(
    config: &RuntimeConfig,
    camera: &CameraStateSnapshot,
    overlay: &OverlayState,
    viewport: ViewportSize,
    meters_per_pixel: f64,
    framebuffer: &mut Framebuffer,
) {
    let style = RenderStyle::default();
    let camera_view = CameraView::new(viewport, camera, meters_per_pixel);
    let rider = camera_view.world_to_screen(camera.focus_world);

    match camera.mode {
        runtime_core::api::CameraMode::Riding => {
            let relative_heading = overlay.rider_heading_rad.unwrap_or(camera.orientation_rad)
                - camera.orientation_rad;
            framebuffer.draw_rotated_mask(
                rider,
                assets::RIDER_MARKER_RIDING,
                relative_heading,
                style.rider_fill_intensity,
            );
        }
        runtime_core::api::CameraMode::Stopped => {
            framebuffer.draw_mask(
                rider,
                assets::RIDER_MARKER_STOPPED,
                style.rider_fill_intensity,
            );
        }
    }

    if !overlay.north_indicator_visible {
        return;
    }

    let indicator_center = runtime_core::api::ScreenPoint::new(
        config.north_indicator_center.x * viewport.width_px as f32,
        config.north_indicator_center.y * viewport.height_px as f32,
    );
    let indicator_intensity = if overlay.north_up_active {
        style.north_indicator_active_intensity
    } else {
        style.north_indicator_idle_intensity
    };
    framebuffer.draw_mask(
        indicator_center,
        assets::NORTH_INDICATOR_BASE,
        indicator_intensity,
    );
    framebuffer.draw_rotated_mask(
        indicator_center,
        assets::NORTH_INDICATOR_NEEDLE,
        -camera.orientation_rad,
        255,
    );
}
