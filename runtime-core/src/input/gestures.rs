use bevy_ecs::prelude::Resource;

use crate::api::{GestureEventKind, ScreenPoint};

use super::contacts::ActiveTouchFrame;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DerivedGesture {
    pub pan_delta_px: ScreenPoint,
    pub pinch_scale: f32,
    pub rotate_delta_rad: f32,
    pub active_gesture: Option<GestureEventKind>,
    pub touch_active: bool,
}

impl Default for DerivedGesture {
    fn default() -> Self {
        Self {
            pan_delta_px: ScreenPoint::new(0.0, 0.0),
            pinch_scale: 1.0,
            rotate_delta_rad: 0.0,
            active_gesture: None,
            touch_active: false,
        }
    }
}

#[derive(Debug, Clone, Resource, Default)]
pub struct GestureState {
    drag_contact_id: Option<u64>,
    drag_start: Option<ScreenPoint>,
    drag_previous: Option<ScreenPoint>,
    drag_active: bool,
    pinch_contact_ids: Option<(u64, u64)>,
    last_pinch_distance: Option<f32>,
    last_pinch_angle_rad: Option<f32>,
}

impl GestureState {
    pub fn derive(
        &mut self,
        current: Option<&ActiveTouchFrame>,
        pan_deadzone_px: f32,
        rotate_deadzone_rad: f32,
    ) -> DerivedGesture {
        let Some(current) = current else {
            self.reset();
            return DerivedGesture::default();
        };

        let active_count = current.active_contact_count();
        if active_count == 0 {
            self.reset();
            return DerivedGesture::default();
        }

        if active_count == 1 {
            return self.derive_single_touch(current, pan_deadzone_px);
        }

        if active_count == 2 {
            return self.derive_two_touch(current, rotate_deadzone_rad);
        }

        self.reset();
        DerivedGesture {
            touch_active: true,
            ..DerivedGesture::default()
        }
    }

    fn derive_single_touch(
        &mut self,
        current: &ActiveTouchFrame,
        pan_deadzone_px: f32,
    ) -> DerivedGesture {
        let Some(contact) = current.contacts.first().copied() else {
            self.reset();
            return DerivedGesture::default();
        };

        if self.drag_contact_id != Some(contact.id) {
            self.reset();
            self.drag_contact_id = Some(contact.id);
            self.drag_start = Some(contact.position);
            self.drag_previous = Some(contact.position);
            return DerivedGesture {
                touch_active: true,
                ..DerivedGesture::default()
            };
        }

        let drag_start = self.drag_start.unwrap_or(contact.position);
        let drag_previous = self.drag_previous.unwrap_or(contact.position);
        let travel = distance_px(drag_start, contact.position);

        if !self.drag_active {
            self.drag_previous = Some(contact.position);
            if travel >= pan_deadzone_px {
                self.drag_active = true;
            }
            return DerivedGesture {
                touch_active: true,
                ..DerivedGesture::default()
            };
        }

        let pan_delta_px = ScreenPoint::new(
            contact.position.x_px - drag_previous.x_px,
            contact.position.y_px - drag_previous.y_px,
        );
        self.drag_previous = Some(contact.position);
        self.clear_pinch();

        DerivedGesture {
            pan_delta_px,
            active_gesture: Some(GestureEventKind::Pan),
            touch_active: true,
            ..DerivedGesture::default()
        }
    }

    fn derive_two_touch(
        &mut self,
        current: &ActiveTouchFrame,
        rotate_deadzone_rad: f32,
    ) -> DerivedGesture {
        let contacts = current.sorted_contacts();
        let [a, b] = contacts.as_slice() else {
            self.reset();
            return DerivedGesture::default();
        };

        let contact_ids = (a.id, b.id);
        let distance = distance_px(a.position, b.position);
        let angle_rad = angle_rad(a.position, b.position);

        if self.pinch_contact_ids != Some(contact_ids) {
            self.reset();
            self.pinch_contact_ids = Some(contact_ids);
            self.last_pinch_distance = Some(distance);
            self.last_pinch_angle_rad = Some(angle_rad);
            return DerivedGesture {
                touch_active: true,
                ..DerivedGesture::default()
            };
        }

        let last_distance = self.last_pinch_distance.unwrap_or(distance);
        let last_angle = self.last_pinch_angle_rad.unwrap_or(angle_rad);
        self.last_pinch_distance = Some(distance);
        self.last_pinch_angle_rad = Some(angle_rad);
        self.clear_drag();

        let pinch_scale = if last_distance > 0.0 {
            distance / last_distance
        } else {
            1.0
        };
        let rotate_delta_rad = normalize_angle(angle_rad - last_angle);
        let rotate_delta_rad = if rotate_delta_rad.abs() >= rotate_deadzone_rad {
            rotate_delta_rad
        } else {
            0.0
        };
        let active_gesture = if rotate_delta_rad != 0.0 {
            Some(GestureEventKind::Rotate)
        } else if (pinch_scale - 1.0).abs() > f32::EPSILON {
            Some(GestureEventKind::Pinch)
        } else {
            None
        };

        DerivedGesture {
            pinch_scale,
            rotate_delta_rad,
            active_gesture,
            touch_active: true,
            ..DerivedGesture::default()
        }
    }

    pub fn reset(&mut self) {
        self.clear_drag();
        self.clear_pinch();
    }

    fn clear_drag(&mut self) {
        self.drag_contact_id = None;
        self.drag_start = None;
        self.drag_previous = None;
        self.drag_active = false;
    }

    fn clear_pinch(&mut self) {
        self.pinch_contact_ids = None;
        self.last_pinch_distance = None;
        self.last_pinch_angle_rad = None;
    }
}

fn distance_px(a: ScreenPoint, b: ScreenPoint) -> f32 {
    (b.x_px - a.x_px).hypot(b.y_px - a.y_px)
}

fn angle_rad(a: ScreenPoint, b: ScreenPoint) -> f32 {
    (b.y_px - a.y_px).atan2(b.x_px - a.x_px)
}

fn normalize_angle(angle: f32) -> f32 {
    let tau = std::f32::consts::TAU;
    let mut normalized = angle;
    while normalized > std::f32::consts::PI {
        normalized -= tau;
    }
    while normalized < -std::f32::consts::PI {
        normalized += tau;
    }
    normalized
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::{TouchContact, TouchContactFrame, TouchPhase};

    fn touch(id: u64, x: f32, y: f32) -> TouchContact {
        TouchContact {
            id,
            phase: TouchPhase::Moved,
            position: ScreenPoint::new(x, y),
            pressure: None,
        }
    }

    fn active_frame(sequence: u64, contacts: Vec<TouchContact>) -> ActiveTouchFrame {
        ActiveTouchFrame::from_touch_frame(Some(
            &TouchContactFrame::new(sequence, contacts).expect("valid touch frame"),
        ))
        .expect("active frame")
    }

    #[test]
    fn drag_waits_for_deadzone() {
        let mut state = GestureState::default();
        let first = active_frame(1, vec![touch(1, 10.0, 10.0)]);
        let second = active_frame(2, vec![touch(1, 14.0, 10.0)]);

        assert!(
            state
                .derive(Some(&first), 8.0, 0.02)
                .active_gesture
                .is_none()
        );
        assert!(
            state
                .derive(Some(&second), 8.0, 0.02)
                .active_gesture
                .is_none()
        );
    }

    #[test]
    fn drag_activates_after_deadzone_and_emits_pan_delta() {
        let mut state = GestureState::default();
        let first = active_frame(1, vec![touch(1, 10.0, 10.0)]);
        let second = active_frame(2, vec![touch(1, 20.0, 10.0)]);
        let third = active_frame(3, vec![touch(1, 27.0, 13.0)]);

        state.derive(Some(&first), 8.0, 0.02);
        state.derive(Some(&second), 8.0, 0.02);
        let gesture = state.derive(Some(&third), 8.0, 0.02);

        assert_eq!(gesture.active_gesture, Some(GestureEventKind::Pan));
        assert_eq!(gesture.pan_delta_px, ScreenPoint::new(7.0, 3.0));
    }

    #[test]
    fn pinch_reports_distance_ratio() {
        let mut state = GestureState::default();
        let first = active_frame(1, vec![touch(1, 0.0, 0.0), touch(2, 10.0, 0.0)]);
        let second = active_frame(2, vec![touch(1, 0.0, 0.0), touch(2, 20.0, 0.0)]);

        state.derive(Some(&first), 8.0, 0.02);
        let gesture = state.derive(Some(&second), 8.0, 0.02);

        assert_eq!(gesture.active_gesture, Some(GestureEventKind::Pinch));
        assert!((gesture.pinch_scale - 2.0).abs() < 1e-6);
    }

    #[test]
    fn rotate_unwraps_across_pi_boundary() {
        let mut state = GestureState::default();
        let first = active_frame(1, vec![touch(1, 0.0, 0.0), touch(2, -10.0, 0.1)]);
        let second = active_frame(2, vec![touch(1, 0.0, 0.0), touch(2, -10.0, -0.1)]);

        state.derive(Some(&first), 8.0, 0.001);
        let gesture = state.derive(Some(&second), 8.0, 0.001);

        assert_eq!(gesture.active_gesture, Some(GestureEventKind::Rotate));
        assert!(gesture.rotate_delta_rad.abs() < 0.05);
    }

    #[test]
    fn contact_count_transition_resets_gesture_state() {
        let mut state = GestureState::default();
        let drag_start = active_frame(1, vec![touch(1, 10.0, 10.0)]);
        let drag_move = active_frame(2, vec![touch(1, 25.0, 10.0)]);
        let pinch_start = active_frame(3, vec![touch(1, 25.0, 10.0), touch(2, 40.0, 10.0)]);
        let single_again = active_frame(4, vec![touch(1, 28.0, 10.0)]);

        state.derive(Some(&drag_start), 8.0, 0.02);
        state.derive(Some(&drag_move), 8.0, 0.02);
        let pinch = state.derive(Some(&pinch_start), 8.0, 0.02);
        let single = state.derive(Some(&single_again), 8.0, 0.02);

        assert!(pinch.active_gesture.is_none());
        assert!(single.active_gesture.is_none());
    }
}
