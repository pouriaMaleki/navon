#[allow(unused_imports)]
use alloc::{vec, vec::Vec, string::{String, ToString}, boxed::Box, format, borrow::ToOwned};

use bevy_ecs::prelude::Resource;

use crate::api::{TouchContact, TouchContactFrame};

#[derive(Debug, Clone, Default)]
pub struct ActiveTouchFrame {
    pub sequence: u64,
    pub contacts: Vec<TouchContact>,
}

impl ActiveTouchFrame {
    pub fn from_touch_frame(frame: Option<&TouchContactFrame>) -> Option<Self> {
        frame.map(|frame| Self {
            sequence: frame.sequence,
            contacts: frame
                .contacts
                .iter()
                .copied()
                .filter(|contact| contact.is_active())
                .collect(),
        })
    }

    pub fn active_contact_count(&self) -> usize {
        self.contacts.len()
    }

    pub fn sorted_contacts(&self) -> Vec<TouchContact> {
        let mut contacts = self.contacts.clone();
        contacts.sort_by_key(|contact| contact.id);
        contacts
    }
}

#[derive(Debug, Clone, Resource, Default)]
pub struct ContactState {
    pub previous: Option<ActiveTouchFrame>,
    pub current: Option<ActiveTouchFrame>,
}

impl ContactState {
    pub fn stage(&mut self, touch: Option<&TouchContactFrame>) {
        self.previous = self.current.take();
        self.current = ActiveTouchFrame::from_touch_frame(touch);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::{ScreenPoint, TouchContact, TouchContactFrame, TouchPhase};

    #[test]
    fn filters_to_active_contacts_only() {
        let frame = TouchContactFrame::new(
            1,
            vec![
                TouchContact {
                    id: 1,
                    phase: TouchPhase::Started,
                    position: ScreenPoint::new(10.0, 20.0),
                    pressure: None,
                },
                TouchContact {
                    id: 2,
                    phase: TouchPhase::Ended,
                    position: ScreenPoint::new(11.0, 20.0),
                    pressure: None,
                },
            ],
        )
        .expect("valid touch frame");

        let active = ActiveTouchFrame::from_touch_frame(Some(&frame)).expect("active frame");
        assert_eq!(active.active_contact_count(), 1);
        assert_eq!(active.contacts[0].id, 1);
    }
}
