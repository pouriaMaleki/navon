use crate::{CameraView, WorldPoint};

pub(crate) struct CameraTransform {
    pub(crate) zoom: f32,
    center_x: f32,
    center_y: f32,
    cos_h: f32,
    sin_h: f32,
    sx_scale: f32,
    sy_scale: f32,
    anchor_x_px: f32,
    anchor_y_px: f32,
}

impl CameraTransform {
    pub(crate) fn new(view: &CameraView, width: usize, height: usize) -> Self {
        let zoom = if view.zoom < 0.2 { 0.2 } else { view.zoom };
        let half_w = (view.base_bounds.width().max(1) as f32) / (2.0 * zoom);
        let half_h = (view.base_bounds.height().max(1) as f32) / (2.0 * zoom);
        let anchor_x = view.player_anchor_x.clamp(0.2, 0.8);
        let anchor_y = view.player_anchor_y.clamp(0.2, 0.86);
        Self {
            zoom,
            center_x: view.center.x as f32,
            center_y: view.center.y as f32,
            cos_h: libm::cosf(view.heading_rad),
            sin_h: libm::sinf(view.heading_rad),
            sx_scale: (width as f32 * 0.5) / half_w,
            sy_scale: (height as f32 * 0.5) / half_h,
            anchor_x_px: width as f32 * anchor_x,
            anchor_y_px: height as f32 * anchor_y,
        }
    }

    pub(crate) fn world_to_screen(&self, p: WorldPoint) -> (i32, i32) {
        let dx = p.x as f32 - self.center_x;
        let dy = p.y as f32 - self.center_y;
        let rx = dx * self.cos_h + dy * self.sin_h;
        let ry = -dx * self.sin_h + dy * self.cos_h;
        let sx = (rx * self.sx_scale) + self.anchor_x_px;
        let sy = (-ry * self.sy_scale) + self.anchor_y_px;
        (sx as i32, sy as i32)
    }

    pub(crate) fn player_anchor_screen(&self) -> (i32, i32) {
        (self.anchor_x_px as i32, self.anchor_y_px as i32)
    }
}

fn compute_out_code(x: i32, y: i32, xmin: i32, xmax: i32, ymin: i32, ymax: i32) -> u8 {
    let mut code = 0_u8;
    if x < xmin {
        code |= 1;
    } else if x > xmax {
        code |= 2;
    }
    if y < ymin {
        code |= 4;
    } else if y > ymax {
        code |= 8;
    }
    code
}

pub(crate) fn clip_line_to_rect(
    mut x0: i32,
    mut y0: i32,
    mut x1: i32,
    mut y1: i32,
    xmin: i32,
    xmax: i32,
    ymin: i32,
    ymax: i32,
) -> Option<(i32, i32, i32, i32)> {
    let mut out0 = compute_out_code(x0, y0, xmin, xmax, ymin, ymax);
    let mut out1 = compute_out_code(x1, y1, xmin, xmax, ymin, ymax);

    loop {
        if out0 == 0 && out1 == 0 {
            return Some((x0, y0, x1, y1));
        }
        if (out0 & out1) != 0 {
            return None;
        }
        let out = if out0 != 0 { out0 } else { out1 };
        let dx = x1 - x0;
        let dy = y1 - y0;
        let (x, y) = if (out & 8) != 0 {
            if dy == 0 {
                return None;
            }
            (x0 + (dx * (ymax - y0)) / dy, ymax)
        } else if (out & 4) != 0 {
            if dy == 0 {
                return None;
            }
            (x0 + (dx * (ymin - y0)) / dy, ymin)
        } else if (out & 2) != 0 {
            if dx == 0 {
                return None;
            }
            (xmax, y0 + (dy * (xmax - x0)) / dx)
        } else {
            if dx == 0 {
                return None;
            }
            (xmin, y0 + (dy * (xmin - x0)) / dx)
        };

        if out == out0 {
            x0 = x;
            y0 = y;
            out0 = compute_out_code(x0, y0, xmin, xmax, ymin, ymax);
        } else {
            x1 = x;
            y1 = y;
            out1 = compute_out_code(x1, y1, xmin, xmax, ymin, ymax);
        }
    }
}
