pub(crate) mod assets;

use runtime_core::api::{
    CameraOrientationMode, CameraStateSnapshot, OverlayState, RuntimeConfig, ScreenPoint,
    ViewportSize,
};

use crate::camera_view::CameraView;
use crate::raster::Framebuffer;
use crate::style::RenderStyle;

const NORTH_INDICATOR_ACK_BASE_RADIUS_PX: f32 = 20.0;
const NORTH_INDICATOR_ACK_EXPANSION_PX: f32 = 12.0;
const TOP_START_ANGLE_RAD: f32 = -std::f32::consts::FRAC_PI_2;

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
                style.rider_fill_color,
            );
        }
        runtime_core::api::CameraMode::Stopped => {
            framebuffer.draw_mask(rider, assets::RIDER_MARKER_STOPPED, style.rider_fill_color);
        }
    }

    if !overlay.north_indicator_visible {
        return;
    }

    let indicator_center = runtime_core::api::ScreenPoint::new(
        config.north_indicator_center.x * viewport.width_px as f32,
        config.north_indicator_center.y * viewport.height_px as f32,
    );
    let indicator_color = match camera.orientation_mode {
        CameraOrientationMode::TravelUpAuto => style.north_indicator_idle_color,
        CameraOrientationMode::HeadingAcquisition => style.north_indicator_acquisition_color,
        CameraOrientationMode::NorthLocked => style.north_indicator_locked_color,
        CameraOrientationMode::StoppedNorthUp | CameraOrientationMode::NorthPreview => {
            style.north_indicator_active_color
        }
    };
    let indicator_base = if matches!(camera.orientation_mode, CameraOrientationMode::NorthLocked) {
        assets::NORTH_INDICATOR_LOCKED_BASE
    } else {
        assets::NORTH_INDICATOR_BASE
    };
    framebuffer.draw_mask(indicator_center, indicator_base, indicator_color);
    framebuffer.draw_rotated_mask(
        indicator_center,
        assets::NORTH_INDICATOR_NEEDLE,
        -camera.orientation_rad,
        style.rider_fill_color,
    );

    if let Some(progress) = overlay.north_preview_progress {
        framebuffer.draw_rotated_mask_radial_progress(
            indicator_center,
            assets::NORTH_INDICATOR_LOCKED_BASE,
            0.0,
            style.north_indicator_locked_color,
            progress,
            TOP_START_ANGLE_RAD,
        );
        framebuffer.draw_rotated_mask(
            indicator_center,
            assets::NORTH_INDICATOR_NEEDLE,
            -camera.orientation_rad,
            style.rider_fill_color,
        );
    }

    if overlay.compass_ack_progress > 0.0 {
        draw_ack_pulse(
            framebuffer,
            indicator_center,
            overlay.compass_ack_progress,
            style.north_indicator_ack_color,
        );
    }
}

fn draw_ack_pulse(
    framebuffer: &mut Framebuffer,
    center: ScreenPoint,
    progress: f32,
    color: crate::raster::Color,
) {
    let clamped = progress.clamp(0.0, 1.0);
    if clamped <= 0.0 {
        return;
    }

    let radius_px =
        NORTH_INDICATOR_ACK_BASE_RADIUS_PX + ((1.0 - clamped) * NORTH_INDICATOR_ACK_EXPANSION_PX);
    let pulse_color = crate::raster::Color::new(
        (f32::from(color.r) * clamped).round().clamp(0.0, 255.0) as u8,
        (f32::from(color.g) * clamped).round().clamp(0.0, 255.0) as u8,
        (f32::from(color.b) * clamped).round().clamp(0.0, 255.0) as u8,
    );
    draw_ring(framebuffer, center, radius_px, 1, pulse_color);
}

fn draw_ring(
    framebuffer: &mut Framebuffer,
    center: ScreenPoint,
    radius_px: f32,
    thickness_px: u8,
    color: crate::raster::Color,
) {
    let segments = 64usize;
    let angle_step = std::f32::consts::TAU / segments as f32;
    for segment in 0..=segments {
        let angle = TOP_START_ANGLE_RAD + (angle_step * segment as f32);
        let x = center.x_px + (radius_px * angle.cos());
        let y = center.y_px + (radius_px * angle.sin());
        framebuffer.stamp_circle(
            x.round() as i32,
            y.round() as i32,
            thickness_px.max(1),
            color,
        );
    }
}
