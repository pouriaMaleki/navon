pub(crate) mod assets;

use runtime_core::api::{
    CameraOrientationMode, CameraStateSnapshot, OverlayState, RuntimeConfig, ScreenPoint,
    ViewportSize,
};
use runtime_core::route::RouteRenderState;

use crate::camera_view::CameraView;
use crate::raster::Framebuffer;
use crate::style::RenderStyle;

const NORTH_INDICATOR_ACK_BASE_RADIUS_PX: f32 = 20.0;
const NORTH_INDICATOR_ACK_EXPANSION_PX: f32 = 12.0;
const TOP_START_ANGLE_RAD: f32 = -std::f32::consts::FRAC_PI_2;
const SPEED_PANEL_HEIGHT_RATIO: f32 = 0.25;
const SPEED_PANEL_UNIT_SCALE_PX: i32 = 5;
const UNIT_GLYPH_WIDTH: usize = 3;
const UNIT_GLYPH_HEIGHT: usize = 5;

pub fn draw_overlay(
    config: &RuntimeConfig,
    camera: &CameraStateSnapshot,
    overlay: &OverlayState,
    route: &RouteRenderState,
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

    if overlay.north_indicator_visible {
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
        let indicator_base =
            if matches!(camera.orientation_mode, CameraOrientationMode::NorthLocked) {
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

    if overlay.speed_panel_visible {
        draw_speed_panel(framebuffer, viewport, overlay, &style);
    }

    if route.off_route {
        draw_off_route_banner(framebuffer, viewport, &style);
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

fn draw_off_route_banner(
    framebuffer: &mut Framebuffer,
    viewport: ViewportSize,
    style: &RenderStyle,
) {
    let scale_px = 2;
    let padding_x = 10;
    let padding_y = 6;
    let spacing_px = 2;
    let gap_px = 4;
    let text = "OFF ROUTE";
    let text_width = measure_banner_text_width(text, scale_px, spacing_px, gap_px);
    let banner_width = (text_width + padding_x * 2).max(104) as u32;
    let banner_height = (7 * scale_px + padding_y * 2) as u32;
    let banner_x = ((viewport.width_px as i32 - banner_width as i32) / 2).max(8);
    let banner_y = 18;

    framebuffer.fill_rect_overwrite(
        banner_x - 2,
        banner_y - 2,
        banner_width + 4,
        banner_height + 4,
        style.off_route_banner_border_color,
    );
    framebuffer.fill_rect_overwrite(
        banner_x,
        banner_y,
        banner_width,
        banner_height,
        style.off_route_banner_background_color,
    );
    draw_banner_text(
        framebuffer,
        text,
        banner_x + padding_x,
        banner_y + padding_y,
        scale_px,
        spacing_px,
        gap_px,
        style.off_route_banner_text_color,
    );
}

fn measure_banner_text_width(text: &str, scale_px: i32, spacing_px: i32, gap_px: i32) -> i32 {
    let mut width = 0;
    let mut first = true;
    for ch in text.chars() {
        if !first {
            width += if ch == ' ' { gap_px } else { spacing_px };
        }
        width += if ch == ' ' {
            3 * scale_px
        } else {
            5 * scale_px
        };
        first = false;
    }
    width
}

fn draw_banner_text(
    framebuffer: &mut Framebuffer,
    text: &str,
    mut x: i32,
    y: i32,
    scale_px: i32,
    spacing_px: i32,
    gap_px: i32,
    color: crate::raster::Color,
) {
    for ch in text.chars() {
        if ch == ' ' {
            x += 3 * scale_px + gap_px;
            continue;
        }
        if let Some(glyph) = banner_glyph(ch) {
            draw_banner_glyph(framebuffer, glyph, x, y, scale_px, color);
        }
        x += 5 * scale_px + spacing_px;
    }
}

fn draw_banner_glyph(
    framebuffer: &mut Framebuffer,
    glyph: [&'static str; 7],
    x: i32,
    y: i32,
    scale_px: i32,
    color: crate::raster::Color,
) {
    for (row_index, row) in glyph.into_iter().enumerate() {
        for (column_index, pixel) in row.chars().enumerate() {
            if pixel != '1' {
                continue;
            }
            framebuffer.fill_rect_overwrite(
                x + column_index as i32 * scale_px,
                y + row_index as i32 * scale_px,
                scale_px as u32,
                scale_px as u32,
                color,
            );
        }
    }
}

fn banner_glyph(ch: char) -> Option<[&'static str; 7]> {
    match ch {
        'E' => Some([
            "11111", "10000", "11110", "10000", "10000", "10000", "11111",
        ]),
        'F' => Some([
            "11111", "10000", "11110", "10000", "10000", "10000", "10000",
        ]),
        'O' => Some([
            "01110", "10001", "10001", "10001", "10001", "10001", "01110",
        ]),
        'R' => Some([
            "11110", "10001", "10001", "11110", "10100", "10010", "10001",
        ]),
        'T' => Some([
            "11111", "00100", "00100", "00100", "00100", "00100", "00100",
        ]),
        'U' => Some([
            "10001", "10001", "10001", "10001", "10001", "10001", "01110",
        ]),
        _ => None,
    }
}

fn draw_speed_panel(
    framebuffer: &mut Framebuffer,
    viewport: ViewportSize,
    overlay: &OverlayState,
    style: &RenderStyle,
) {
    let panel_top = ((viewport.height_px as f32) * (1.0 - SPEED_PANEL_HEIGHT_RATIO)).round() as i32;
    let panel_height = viewport.height_px.saturating_sub(panel_top.max(0) as u32);
    framebuffer.fill_rect_overwrite(
        0,
        panel_top,
        viewport.width_px,
        panel_height,
        style.speed_panel_background_color,
    );

    let digits = overlay.speed_display_value.to_string();
    let digit_height = ((panel_height as f32) * 0.48).round().max(36.0) as i32;
    let digit_width = (digit_height as f32 * 0.56).round() as i32;
    let digit_thickness = (digit_height / 7).max(4);
    let digit_gap = (digit_width / 5).max(8);
    let total_digits_width =
        digits.len() as i32 * digit_width + ((digits.len().saturating_sub(1)) as i32 * digit_gap);
    let digits_origin_x = ((viewport.width_px as i32 - total_digits_width) / 2).max(0);
    let digits_origin_y = panel_top + ((panel_height as i32 - digit_height) / 2) - 18;

    for (index, digit) in digits.chars().enumerate() {
        draw_segment_digit(
            framebuffer,
            digit,
            digits_origin_x + index as i32 * (digit_width + digit_gap),
            digits_origin_y,
            digit_width,
            digit_height,
            digit_thickness,
            style.speed_panel_text_color,
        );
    }

    let unit_patterns = match overlay.speed_unit {
        runtime_core::api::SpeedUnit::Kph => [UNIT_GLYPH_K, UNIT_GLYPH_P, UNIT_GLYPH_H],
        runtime_core::api::SpeedUnit::Mph => [UNIT_GLYPH_M, UNIT_GLYPH_P, UNIT_GLYPH_H],
    };
    let unit_gap = SPEED_PANEL_UNIT_SCALE_PX;
    let unit_total_width =
        (unit_patterns.len() as i32 * UNIT_GLYPH_WIDTH as i32 * SPEED_PANEL_UNIT_SCALE_PX)
            + ((unit_patterns.len().saturating_sub(1) as i32) * unit_gap);
    let unit_origin_x = ((viewport.width_px as i32 - unit_total_width) / 2).max(0);
    let unit_origin_y =
        panel_top + panel_height as i32 - UNIT_GLYPH_HEIGHT as i32 * SPEED_PANEL_UNIT_SCALE_PX - 18;
    for (index, glyph) in unit_patterns.into_iter().enumerate() {
        draw_unit_glyph(
            framebuffer,
            glyph,
            unit_origin_x
                + index as i32 * (UNIT_GLYPH_WIDTH as i32 * SPEED_PANEL_UNIT_SCALE_PX + unit_gap),
            unit_origin_y,
            SPEED_PANEL_UNIT_SCALE_PX,
            style.speed_panel_text_color,
        );
    }
}

fn draw_segment_digit(
    framebuffer: &mut Framebuffer,
    digit: char,
    x: i32,
    y: i32,
    width: i32,
    height: i32,
    thickness: i32,
    color: crate::raster::Color,
) {
    let vertical_height = ((height - thickness * 3) / 2).max(1);
    let top_y = y;
    let middle_y = y + thickness + vertical_height;
    let bottom_y = y + thickness * 2 + vertical_height * 2;
    let left_x = x;
    let right_x = x + width - thickness;

    let segments = match digit {
        '0' => [true, true, true, false, true, true, true],
        '1' => [false, false, true, false, false, true, false],
        '2' => [true, false, true, true, true, false, true],
        '3' => [true, false, true, true, false, true, true],
        '4' => [false, true, true, true, false, true, false],
        '5' => [true, true, false, true, false, true, true],
        '6' => [true, true, false, true, true, true, true],
        '7' => [true, false, true, false, false, true, false],
        '8' => [true, true, true, true, true, true, true],
        '9' => [true, true, true, true, false, true, true],
        _ => [false, false, false, false, false, false, false],
    };

    if segments[0] {
        framebuffer.fill_rect(x, top_y, width as u32, thickness as u32, color);
    }
    if segments[1] {
        framebuffer.fill_rect(
            left_x,
            top_y + thickness,
            thickness as u32,
            vertical_height as u32,
            color,
        );
    }
    if segments[2] {
        framebuffer.fill_rect(
            right_x,
            top_y + thickness,
            thickness as u32,
            vertical_height as u32,
            color,
        );
    }
    if segments[3] {
        framebuffer.fill_rect(x, middle_y, width as u32, thickness as u32, color);
    }
    if segments[4] {
        framebuffer.fill_rect(
            left_x,
            middle_y + thickness,
            thickness as u32,
            vertical_height as u32,
            color,
        );
    }
    if segments[5] {
        framebuffer.fill_rect(
            right_x,
            middle_y + thickness,
            thickness as u32,
            vertical_height as u32,
            color,
        );
    }
    if segments[6] {
        framebuffer.fill_rect(x, bottom_y, width as u32, thickness as u32, color);
    }
}

fn draw_unit_glyph(
    framebuffer: &mut Framebuffer,
    glyph: [&'static str; UNIT_GLYPH_HEIGHT],
    x: i32,
    y: i32,
    scale_px: i32,
    color: crate::raster::Color,
) {
    for (row_index, row) in glyph.into_iter().enumerate() {
        for (column_index, pixel) in row.chars().enumerate() {
            if pixel != '1' {
                continue;
            }
            framebuffer.fill_rect(
                x + column_index as i32 * scale_px,
                y + row_index as i32 * scale_px,
                scale_px as u32,
                scale_px as u32,
                color,
            );
        }
    }
}

const UNIT_GLYPH_K: [&str; UNIT_GLYPH_HEIGHT] = ["101", "101", "110", "101", "101"];
const UNIT_GLYPH_M: [&str; UNIT_GLYPH_HEIGHT] = ["101", "111", "111", "101", "101"];
const UNIT_GLYPH_P: [&str; UNIT_GLYPH_HEIGHT] = ["110", "101", "110", "100", "100"];
const UNIT_GLYPH_H: [&str; UNIT_GLYPH_HEIGHT] = ["101", "101", "111", "101", "101"];
