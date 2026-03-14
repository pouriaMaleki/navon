use runtime_core::api::{CameraStateSnapshot, OverlayState, RuntimeConfig, ViewportSize};

use crate::camera_view::CameraView;
use crate::raster::Framebuffer;
use crate::style::RenderStyle;

pub fn draw_overlay(
    config: &RuntimeConfig,
    camera: &CameraStateSnapshot,
    overlay: &OverlayState,
    viewport: ViewportSize,
    framebuffer: &mut Framebuffer,
) {
    let style = RenderStyle::default();
    let camera_view = CameraView::new(viewport, camera);
    let rider = camera_view.world_to_screen(camera.focus_world);
    framebuffer.stamp_circle(
        rider.x_px.round() as i32,
        rider.y_px.round() as i32,
        match camera.mode {
            runtime_core::api::CameraMode::Riding => 5,
            runtime_core::api::CameraMode::Stopped => 7,
        },
        style.rider_fill_intensity,
    );

    if let Some(rider_heading_rad) = overlay.rider_heading_rad {
        let relative_heading = rider_heading_rad - camera.orientation_rad;
        let heading_length = 12.0_f32;
        let heading_tip_x = rider.x_px + (relative_heading.sin() * heading_length);
        let heading_tip_y = rider.y_px - (relative_heading.cos() * heading_length);
        framebuffer.draw_line(
            rider,
            runtime_core::api::ScreenPoint::new(heading_tip_x, heading_tip_y),
            style.rider_heading_intensity,
            2,
        );
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
    framebuffer.stamp_circle(
        indicator_center.x_px.round() as i32,
        indicator_center.y_px.round() as i32,
        10,
        indicator_intensity,
    );

    let north_angle = -camera.orientation_rad;
    let needle_length = 14.0_f32;
    let needle_tip = runtime_core::api::ScreenPoint::new(
        indicator_center.x_px + (north_angle.sin() * needle_length),
        indicator_center.y_px - (north_angle.cos() * needle_length),
    );
    framebuffer.draw_line(indicator_center, needle_tip, 255, 2);
}
