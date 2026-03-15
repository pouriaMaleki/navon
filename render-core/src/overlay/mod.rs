mod assets;

use runtime_core::api::{
    CameraOrientationMode, CameraStateSnapshot, OverlayState, RuntimeConfig, ScreenPoint,
    ViewportSize,
};

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
    let indicator_intensity = match camera.orientation_mode {
        CameraOrientationMode::TravelUpAuto => style.north_indicator_idle_intensity,
        CameraOrientationMode::HeadingAcquisition => style.north_indicator_acquisition_intensity,
        CameraOrientationMode::NorthLocked => style.north_indicator_locked_intensity,
        CameraOrientationMode::StoppedNorthUp | CameraOrientationMode::NorthPreview => {
            style.north_indicator_active_intensity
        }
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

    if let Some(progress) = overlay.north_preview_progress {
        draw_progress_ring(
            framebuffer,
            indicator_center,
            12.0,
            2,
            progress,
            style.north_indicator_ring_intensity,
        );
    }

    if matches!(camera.orientation_mode, CameraOrientationMode::NorthLocked) {
        draw_progress_ring(
            framebuffer,
            indicator_center,
            12.0,
            2,
            1.0,
            style.north_indicator_locked_intensity,
        );
    }

    if overlay.compass_ack_progress > 0.0 {
        draw_ack_pulse(
            framebuffer,
            indicator_center,
            overlay.compass_ack_progress,
            style.north_indicator_ack_intensity,
        );
    }
}

fn draw_progress_ring(
    framebuffer: &mut Framebuffer,
    center: ScreenPoint,
    radius_px: f32,
    thickness_px: u8,
    progress: f32,
    intensity: u8,
) {
    let clamped_progress = progress.clamp(0.0, 1.0);
    if clamped_progress <= 0.0 {
        return;
    }

    let segments = ((64.0 * clamped_progress).ceil() as usize).max(1);
    let start_angle = -std::f32::consts::FRAC_PI_2;
    let end_angle = start_angle + (std::f32::consts::TAU * clamped_progress);
    let angle_step = (end_angle - start_angle) / segments as f32;
    for segment in 0..=segments {
        let angle = start_angle + (angle_step * segment as f32);
        let x = center.x_px + (radius_px * angle.cos());
        let y = center.y_px + (radius_px * angle.sin());
        framebuffer.stamp_circle(
            x.round() as i32,
            y.round() as i32,
            thickness_px.max(1),
            intensity,
        );
    }
}

fn draw_ack_pulse(
    framebuffer: &mut Framebuffer,
    center: ScreenPoint,
    progress: f32,
    intensity: u8,
) {
    let clamped = progress.clamp(0.0, 1.0);
    if clamped <= 0.0 {
        return;
    }

    let radius_px = 10.0 + ((1.0 - clamped) * 8.0);
    let pulse_intensity = (f32::from(intensity) * clamped).round().clamp(0.0, 255.0) as u8;
    draw_progress_ring(framebuffer, center, radius_px, 1, 1.0, pulse_intensity);
}
