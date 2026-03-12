use crate::{FrameBuffer, WorldBounds, WorldPoint};

pub(crate) fn world_to_screen(
    p: WorldPoint,
    bounds: WorldBounds,
    width: usize,
    height: usize,
) -> (i32, i32) {
    let bw = bounds.width().max(1) as i32;
    let bh = bounds.height().max(1) as i32;
    let px = (p.x - bounds.min_x) as i32;
    let py = (p.y - bounds.min_y) as i32;

    let sx = (px * (width.saturating_sub(1) as i32)) / bw;
    let sy = ((bh - py) * (height.saturating_sub(1) as i32)) / bh;
    (sx, sy)
}

pub(crate) fn draw_line(
    frame: &mut FrameBuffer<'_>,
    from: (i32, i32),
    to: (i32, i32),
    value: u8,
    r: i32,
) {
    let (mut x0, mut y0) = from;
    let (x1, y1) = to;
    let dx = (x1 - x0).abs();
    let sx = if x0 < x1 { 1 } else { -1 };
    let dy = -(y1 - y0).abs();
    let sy = if y0 < y1 { 1 } else { -1 };
    let mut err = dx + dy;

    loop {
        stamp_circle(frame, x0, y0, r, value);
        if x0 == x1 && y0 == y1 {
            break;
        }
        let e2 = 2 * err;
        if e2 >= dy {
            err += dy;
            x0 += sx;
        }
        if e2 <= dx {
            err += dx;
            y0 += sy;
        }
    }
}

pub(crate) fn stamp_circle(frame: &mut FrameBuffer<'_>, cx: i32, cy: i32, r: i32, value: u8) {
    for dy in -r..=r {
        for dx in -r..=r {
            if dx * dx + dy * dy <= r * r {
                frame.set_pixel_checked(cx + dx, cy + dy, value);
            }
        }
    }
}

pub(crate) fn fill_circle(frame: &mut FrameBuffer<'_>, cx: i32, cy: i32, r: i32, value: u8) {
    for dy in -r..=r {
        for dx in -r..=r {
            if dx * dx + dy * dy > r * r {
                continue;
            }
            let x = cx + dx;
            let y = cy + dy;
            if x < 0 || y < 0 {
                continue;
            }
            let ux = x as usize;
            let uy = y as usize;
            if ux >= frame.width || uy >= frame.height {
                continue;
            }
            let idx = uy * frame.width + ux;
            frame.pixels[idx] = value;
        }
    }
}
