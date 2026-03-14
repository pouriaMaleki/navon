use std::time::Duration;

use bevy_ecs::prelude::Resource;

use crate::api::{ScreenPoint, TapEvent};

use super::contacts::ActiveTouchFrame;

#[derive(Debug, Clone, Copy)]
struct TapCandidate {
    contact_id: u64,
    started_at: Duration,
    start_position: ScreenPoint,
    last_position: ScreenPoint,
}

#[derive(Debug, Clone, Resource, Default)]
pub struct TapState {
    candidate: Option<TapCandidate>,
}

impl TapState {
    pub fn update(
        &mut self,
        previous: Option<&ActiveTouchFrame>,
        current: Option<&ActiveTouchFrame>,
        total_time: Duration,
        tap_max_duration: Duration,
        tap_max_travel_px: f32,
    ) -> Option<TapEvent> {
        let previous_active_count = previous
            .map(|frame| frame.active_contact_count())
            .unwrap_or(0);
        let previous_single = previous.and_then(single_contact);
        let current_single = current.and_then(single_contact);

        if let Some(current_contact) = current_single {
            match self.candidate {
                Some(mut candidate) if candidate.contact_id == current_contact.id => {
                    candidate.last_position = current_contact.position;
                    if distance_px(candidate.start_position, current_contact.position)
                        > tap_max_travel_px
                    {
                        self.candidate = None;
                    } else {
                        self.candidate = Some(candidate);
                    }
                }
                _ => {
                    self.candidate = if previous_active_count == 0 {
                        Some(TapCandidate {
                            contact_id: current_contact.id,
                            started_at: total_time,
                            start_position: current_contact.position,
                            last_position: current_contact.position,
                        })
                    } else {
                        None
                    };
                }
            }
            return None;
        }

        let tap = match (self.candidate.take(), previous_single, current_single) {
            (Some(candidate), Some(previous_contact), None)
                if candidate.contact_id == previous_contact.id
                    && total_time.saturating_sub(candidate.started_at) <= tap_max_duration
                    && distance_px(candidate.start_position, candidate.last_position)
                        <= tap_max_travel_px =>
            {
                Some(TapEvent {
                    position: candidate.last_position,
                })
            }
            _ => None,
        };

        if current
            .map(|frame| frame.active_contact_count() != 1)
            .unwrap_or(true)
        {
            self.candidate = None;
        }

        tap
    }
}

fn single_contact(frame: &ActiveTouchFrame) -> Option<crate::api::TouchContact> {
    match frame.contacts.as_slice() {
        [contact] => Some(*contact),
        _ => None,
    }
}

fn distance_px(a: ScreenPoint, b: ScreenPoint) -> f32 {
    (b.x_px - a.x_px).hypot(b.y_px - a.y_px)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::{TouchContact, TouchContactFrame, TouchPhase};

    fn touch_frame(sequence: u64, contacts: Vec<TouchContact>) -> ActiveTouchFrame {
        ActiveTouchFrame::from_touch_frame(Some(
            &TouchContactFrame::new(sequence, contacts).expect("valid touch frame"),
        ))
        .expect("active frame")
    }

    fn touch(id: u64, x: f32, y: f32) -> TouchContact {
        TouchContact {
            id,
            phase: TouchPhase::Moved,
            position: ScreenPoint::new(x, y),
            pressure: None,
        }
    }

    #[test]
    fn tap_succeeds_for_short_low_travel_sequence() {
        let mut state = TapState::default();
        let first = touch_frame(1, vec![touch(1, 20.0, 20.0)]);
        let second = touch_frame(2, vec![touch(1, 24.0, 20.0)]);

        state.update(
            None,
            Some(&first),
            Duration::from_millis(10),
            Duration::from_millis(260),
            10.0,
        );
        state.update(
            Some(&first),
            Some(&second),
            Duration::from_millis(40),
            Duration::from_millis(260),
            10.0,
        );
        let tap = state.update(
            Some(&second),
            None,
            Duration::from_millis(80),
            Duration::from_millis(260),
            10.0,
        );

        assert_eq!(tap.expect("tap").position, ScreenPoint::new(24.0, 20.0));
    }

    #[test]
    fn tap_fails_when_travel_exceeds_limit() {
        let mut state = TapState::default();
        let first = touch_frame(1, vec![touch(1, 20.0, 20.0)]);
        let second = touch_frame(2, vec![touch(1, 40.0, 20.0)]);

        state.update(
            None,
            Some(&first),
            Duration::from_millis(10),
            Duration::from_millis(260),
            10.0,
        );
        state.update(
            Some(&first),
            Some(&second),
            Duration::from_millis(40),
            Duration::from_millis(260),
            10.0,
        );
        let tap = state.update(
            Some(&second),
            None,
            Duration::from_millis(80),
            Duration::from_millis(260),
            10.0,
        );

        assert!(tap.is_none());
    }

    #[test]
    fn tap_fails_when_press_takes_too_long() {
        let mut state = TapState::default();
        let first = touch_frame(1, vec![touch(1, 20.0, 20.0)]);
        let second = touch_frame(2, vec![touch(1, 22.0, 20.0)]);

        state.update(
            None,
            Some(&first),
            Duration::from_millis(10),
            Duration::from_millis(260),
            10.0,
        );
        state.update(
            Some(&first),
            Some(&second),
            Duration::from_millis(200),
            Duration::from_millis(260),
            10.0,
        );
        let tap = state.update(
            Some(&second),
            None,
            Duration::from_millis(400),
            Duration::from_millis(260),
            10.0,
        );

        assert!(tap.is_none());
    }

    #[test]
    fn tap_is_not_started_from_multi_touch_collapse() {
        let mut state = TapState::default();
        let multi = touch_frame(1, vec![touch(1, 20.0, 20.0), touch(2, 40.0, 20.0)]);
        let single_tail = touch_frame(2, vec![touch(1, 22.0, 20.0)]);

        state.update(
            None,
            Some(&multi),
            Duration::from_millis(10),
            Duration::from_millis(260),
            10.0,
        );
        state.update(
            Some(&multi),
            Some(&single_tail),
            Duration::from_millis(40),
            Duration::from_millis(260),
            10.0,
        );
        let tap = state.update(
            Some(&single_tail),
            None,
            Duration::from_millis(80),
            Duration::from_millis(260),
            10.0,
        );

        assert!(tap.is_none());
    }
}
