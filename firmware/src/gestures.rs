use crate::board_config::{
    TOUCH_MAX_TRACKED_POINTS, TOUCH_PAN_DEADZONE_PX, TOUCH_TAP_MAX_MS, TOUCH_TAP_MOVE_PX,
};
use libm::atan2f;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct NormalizedPoint {
    pub x: f32,
    pub y: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TouchPoint {
    pub id: u8,
    pub x: u16,
    pub y: u16,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TouchFrame {
    pub points: [Option<TouchPoint>; TOUCH_MAX_TRACKED_POINTS],
}

impl TouchFrame {
    pub const fn empty() -> Self {
        Self {
            points: [None, None],
        }
    }

    pub fn contact_count(&self) -> usize {
        self.points.iter().flatten().count()
    }

    pub fn point(&self, index: usize) -> Option<TouchPoint> {
        self.points.get(index).copied().flatten()
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GestureUpdate {
    pub pan_dx_px: f32,
    pub pan_dy_px: f32,
    pub zoom_scale: f32,
    pub rotate_delta_rad: f32,
    pub tap: Option<NormalizedPoint>,
}

impl Default for GestureUpdate {
    fn default() -> Self {
        Self {
            pan_dx_px: 0.0,
            pan_dy_px: 0.0,
            zoom_scale: 1.0,
            rotate_delta_rad: 0.0,
            tap: None,
        }
    }
}

pub struct GestureRecognizer {
    prev: TouchFrame,
    down_started_ms: u32,
    tap_origin: Option<TouchPoint>,
    tap_candidate: bool,
    panel_width: f32,
    panel_height: f32,
}

impl GestureRecognizer {
    pub fn new(panel_width: u16, panel_height: u16) -> Self {
        Self {
            prev: TouchFrame::empty(),
            down_started_ms: 0,
            tap_origin: None,
            tap_candidate: false,
            panel_width: panel_width.max(1) as f32,
            panel_height: panel_height.max(1) as f32,
        }
    }

    pub fn update(&mut self, frame: TouchFrame, now_ms: u32) -> GestureUpdate {
        let mut out = GestureUpdate::default();
        let prev_count = self.prev.contact_count();
        let curr_count = frame.contact_count();

        match curr_count {
            0 => {
                if prev_count == 1 && self.tap_candidate {
                    if let Some(origin) = self.tap_origin {
                        let elapsed = now_ms.saturating_sub(self.down_started_ms);
                        if elapsed <= TOUCH_TAP_MAX_MS {
                            out.tap = Some(self.normalize(origin));
                        }
                    }
                }
                self.tap_origin = None;
                self.tap_candidate = false;
            }
            1 => {
                let Some(curr) = frame.point(0) else {
                    self.prev = frame;
                    return out;
                };
                if prev_count != 1 {
                    self.down_started_ms = now_ms;
                    self.tap_origin = Some(curr);
                    self.tap_candidate = true;
                } else if let Some(prev) = self.prev.point(0) {
                    if prev.id == curr.id {
                        let dx = curr.x as f32 - prev.x as f32;
                        let dy = curr.y as f32 - prev.y as f32;
                        if dx.abs() > TOUCH_PAN_DEADZONE_PX || dy.abs() > TOUCH_PAN_DEADZONE_PX {
                            out.pan_dx_px = dx;
                            out.pan_dy_px = dy;
                        }
                        if let Some(origin) = self.tap_origin {
                            let travel = distance_px(origin, curr);
                            if travel > TOUCH_TAP_MOVE_PX {
                                self.tap_candidate = false;
                            }
                        }
                    } else {
                        self.down_started_ms = now_ms;
                        self.tap_origin = Some(curr);
                        self.tap_candidate = true;
                    }
                }
            }
            _ => {
                self.tap_candidate = false;
                if let (Some(prev_a), Some(prev_b), Some(curr_a), Some(curr_b)) = (
                    self.prev.point(0),
                    self.prev.point(1),
                    frame.point(0),
                    frame.point(1),
                ) {
                    let prev_distance = distance_px(prev_a, prev_b);
                    let curr_distance = distance_px(curr_a, curr_b);
                    if prev_distance > 0.0 && curr_distance > 0.0 {
                        out.zoom_scale = curr_distance / prev_distance;
                    }
                    let prev_angle = angle_rad(prev_a, prev_b);
                    let curr_angle = angle_rad(curr_a, curr_b);
                    out.rotate_delta_rad = normalize_angle(curr_angle - prev_angle);
                }
            }
        }

        self.prev = frame;
        out
    }

    fn normalize(&self, point: TouchPoint) -> NormalizedPoint {
        NormalizedPoint {
            x: (point.x as f32 / self.panel_width).clamp(0.0, 1.0),
            y: (point.y as f32 / self.panel_height).clamp(0.0, 1.0),
        }
    }
}

fn distance_px(a: TouchPoint, b: TouchPoint) -> f32 {
    let dx = a.x as f32 - b.x as f32;
    let dy = a.y as f32 - b.y as f32;
    libm::sqrtf((dx * dx) + (dy * dy))
}

fn angle_rad(a: TouchPoint, b: TouchPoint) -> f32 {
    atan2f(b.y as f32 - a.y as f32, b.x as f32 - a.x as f32)
}

fn normalize_angle(angle: f32) -> f32 {
    let mut out = angle;
    let two_pi = core::f32::consts::PI * 2.0;
    while out > core::f32::consts::PI {
        out -= two_pi;
    }
    while out < -core::f32::consts::PI {
        out += two_pi;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn point(id: u8, x: u16, y: u16) -> TouchPoint {
        TouchPoint { id, x, y }
    }

    #[test]
    fn single_touch_drag_produces_pan() {
        let mut gestures = GestureRecognizer::new(800, 800);
        let _ = gestures.update(
            TouchFrame {
                points: [Some(point(1, 100, 120)), None],
            },
            10,
        );
        let update = gestures.update(
            TouchFrame {
                points: [Some(point(1, 110, 126)), None],
            },
            20,
        );
        assert!(update.pan_dx_px > 0.0);
        assert!(update.pan_dy_px > 0.0);
    }

    #[test]
    fn quick_release_becomes_tap() {
        let mut gestures = GestureRecognizer::new(800, 800);
        let _ = gestures.update(
            TouchFrame {
                points: [Some(point(1, 640, 120)), None],
            },
            100,
        );
        let update = gestures.update(TouchFrame::empty(), 180);
        assert_eq!(update.tap, Some(NormalizedPoint { x: 0.8, y: 0.15 }));
    }

    #[test]
    fn dual_touch_produces_zoom_and_rotate() {
        let mut gestures = GestureRecognizer::new(800, 800);
        let _ = gestures.update(
            TouchFrame {
                points: [Some(point(1, 200, 200)), Some(point(2, 400, 200))],
            },
            0,
        );
        let update = gestures.update(
            TouchFrame {
                points: [Some(point(1, 200, 220)), Some(point(2, 430, 180))],
            },
            16,
        );
        assert!(update.zoom_scale > 1.0);
        assert!(update.rotate_delta_rad.abs() > 0.01);
    }
}
