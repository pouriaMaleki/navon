use crate::raster::{draw_line, fill_circle, stamp_circle, world_to_screen};
use crate::{FrameBuffer, WorldBounds, WorldPoint};

pub(crate) fn line_style_dark_profile(
    base_intensity: u8,
    thickness: u8,
    interaction_active: bool,
) -> (u8, i32) {
    let raw_thickness = thickness.max(1);
    let (intensity, radius) = match raw_thickness {
        3.. => (base_intensity.max(236), raw_thickness as i32 + 1),
        2 => (base_intensity.max(214), 2),
        _ => (base_intensity.saturating_sub(34).max(118), 1),
    };

    if interaction_active && raw_thickness == 1 {
        (intensity.saturating_sub(12), 1)
    } else {
        (intensity, radius)
    }
}

pub(crate) fn draw_player(frame: &mut FrameBuffer<'_>, player: WorldPoint, bounds: WorldBounds) {
    let (x, y) = world_to_screen(player, bounds, frame.width, frame.height);
    draw_player_screen(frame, (x, y), 0.0, false);
}

pub(crate) fn draw_player_screen(
    frame: &mut FrameBuffer<'_>,
    p: (i32, i32),
    heading_rad: f32,
    riding_mode: bool,
) {
    let (x, y) = p;
    if riding_mode {
        // Riding marker: brighter glow + forward pointer.
        stamp_circle(frame, x, y, 9, 205);
        stamp_circle(frame, x, y, 6, 238);
        stamp_circle(frame, x, y, 3, 252);

        let nx = libm::sinf(heading_rad);
        let ny = -libm::cosf(heading_rad);
        for step in 1..=8 {
            let fx = x + (nx * (step as f32) * 1.25) as i32;
            let fy = y + (ny * (step as f32) * 1.25) as i32;
            let value = if step < 4 { 252 } else { 238 };
            stamp_circle(frame, fx, fy, 1, value);
        }
    } else {
        // Stopped marker: larger static marker for easy map readability.
        stamp_circle(frame, x, y, 8, 215);
        stamp_circle(frame, x, y, 5, 242);
        stamp_circle(frame, x, y, 2, 255);
    }
}

pub(crate) fn draw_north_indicator(frame: &mut FrameBuffer<'_>, map_heading_rad: f32) {
    let (cx, cy, r) = north_indicator_geometry(frame.width, frame.height);
    // Fill backdrop with explicit writes so it remains visible over bright geometry.
    fill_circle(frame, cx, cy, r, 18);
    stamp_circle(frame, cx, cy, r, 96);
    stamp_circle(frame, cx, cy, (r - 2).max(1), 24);

    // Arrow points towards world north relative to current map rotation.
    let angle = -core::f32::consts::FRAC_PI_2 + map_heading_rad;
    let nx = libm::cosf(angle);
    let ny = libm::sinf(angle);
    let tip_x = cx + (nx * (r as f32 - 4.0)) as i32;
    let tip_y = cy + (ny * (r as f32 - 4.0)) as i32;
    draw_line(frame, (cx, cy), (tip_x, tip_y), 245, 1);

    let left = angle + 2.55;
    let right = angle - 2.55;
    let wing = (r as f32 * 0.45).max(3.0);
    let lx = tip_x + (libm::cosf(left) * wing) as i32;
    let ly = tip_y + (libm::sinf(left) * wing) as i32;
    let rx = tip_x + (libm::cosf(right) * wing) as i32;
    let ry = tip_y + (libm::sinf(right) * wing) as i32;
    draw_line(frame, (tip_x, tip_y), (lx, ly), 245, 1);
    draw_line(frame, (tip_x, tip_y), (rx, ry), 245, 1);
}

pub(crate) fn north_indicator_geometry(width: usize, height: usize) -> (i32, i32, i32) {
    let min_side = width.min(height) as i32;
    let frame_cx = (width / 2) as i32;
    let frame_cy = (height / 2) as i32;
    let frame_r = (min_side / 2) - 8;
    let indicator_r = (min_side / 12).clamp(24, 36);
    let radial = (frame_r - indicator_r - 10).max(indicator_r + 1);
    // 45-degree placement in top-right area, always inside round screen mask.
    let offset = (radial as f32 * 0.70710677) as i32;
    (frame_cx + offset, frame_cy - offset, indicator_r)
}

pub(crate) fn apply_device_style(frame: &mut FrameBuffer<'_>) {
    let cx = (frame.width / 2) as i32;
    let cy = (frame.height / 2) as i32;
    let radius = ((frame.width.min(frame.height) as i32) / 2) - 8;
    let inner = (radius - 6).max(1);

    for y in 0..frame.height as i32 {
        for x in 0..frame.width as i32 {
            let dx = x - cx;
            let dy = y - cy;
            let d2 = dx * dx + dy * dy;
            let idx = (y as usize) * frame.width + (x as usize);
            if d2 > radius * radius {
                frame.pixels[idx] = 0;
            } else if d2 >= inner * inner {
                frame.pixels[idx] = frame.pixels[idx].max(210);
            }
        }
    }

    for y in (cy - radius)..(cy - radius + 14) {
        for x in (cx - 2)..=(cx + 2) {
            frame.set_pixel_checked(x, y, 255);
        }
    }
}
