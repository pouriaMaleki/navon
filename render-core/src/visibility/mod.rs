use runtime_core::api::{ScreenPoint, ViewportSize};

const INSIDE: u8 = 0;
const LEFT: u8 = 1;
const RIGHT: u8 = 2;
const BOTTOM: u8 = 4;
const TOP: u8 = 8;

pub fn clip_segment_to_viewport(
    mut from: ScreenPoint,
    mut to: ScreenPoint,
    viewport: ViewportSize,
) -> Option<(ScreenPoint, ScreenPoint)> {
    if viewport.is_empty() {
        return None;
    }

    let x_max = viewport.width_px as f32 - 1.0;
    let y_max = viewport.height_px as f32 - 1.0;

    loop {
        let from_code = out_code(from, x_max, y_max);
        let to_code = out_code(to, x_max, y_max);

        if from_code | to_code == INSIDE {
            return Some((from, to));
        }
        if from_code & to_code != INSIDE {
            return None;
        }

        let clip_code = if from_code != INSIDE {
            from_code
        } else {
            to_code
        };
        let (next_x, next_y) = if clip_code & TOP != 0 {
            let x = from.x_px + ((to.x_px - from.x_px) * (0.0 - from.y_px) / (to.y_px - from.y_px));
            (x, 0.0)
        } else if clip_code & BOTTOM != 0 {
            let y = y_max;
            let x = from.x_px + ((to.x_px - from.x_px) * (y - from.y_px) / (to.y_px - from.y_px));
            (x, y)
        } else if clip_code & RIGHT != 0 {
            let x = x_max;
            let y = from.y_px + ((to.y_px - from.y_px) * (x - from.x_px) / (to.x_px - from.x_px));
            (x, y)
        } else {
            let y = from.y_px + ((to.y_px - from.y_px) * (0.0 - from.x_px) / (to.x_px - from.x_px));
            (0.0, y)
        };

        if clip_code == from_code {
            from = ScreenPoint::new(next_x, next_y);
        } else {
            to = ScreenPoint::new(next_x, next_y);
        }
    }
}

fn out_code(point: ScreenPoint, x_max: f32, y_max: f32) -> u8 {
    let mut code = INSIDE;
    if point.x_px < 0.0 {
        code |= LEFT;
    } else if point.x_px > x_max {
        code |= RIGHT;
    }
    if point.y_px < 0.0 {
        code |= TOP;
    } else if point.y_px > y_max {
        code |= BOTTOM;
    }
    code
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clips_line_against_viewport_bounds() {
        let clipped = clip_segment_to_viewport(
            ScreenPoint::new(-10.0, 10.0),
            ScreenPoint::new(30.0, 10.0),
            ViewportSize::new(20, 20),
        )
        .expect("segment should clip");

        assert!((clipped.0.x_px - 0.0).abs() < 0.01);
        assert!((clipped.1.x_px - 19.0).abs() < 0.01);
    }
}
