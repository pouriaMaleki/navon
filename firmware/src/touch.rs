use runtime_core::api::{
    ScreenPoint, TouchContact, TouchContactFrame, TouchContactFrameError, TouchPhase,
};

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RawTouchContact {
    pub id: u64,
    pub phase: TouchPhase,
    pub position: ScreenPoint,
    pub pressure: Option<f32>,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct TouchInput {
    pub sequence: u64,
    pub contacts: Vec<RawTouchContact>,
}

impl TouchInput {
    pub fn into_runtime_frame(self) -> Result<TouchContactFrame, TouchContactFrameError> {
        let contacts = self
            .contacts
            .into_iter()
            .map(|contact| TouchContact {
                id: contact.id,
                phase: contact.phase,
                position: contact.position,
                pressure: contact.pressure,
            })
            .collect();
        TouchContactFrame::new(self.sequence, contacts)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn converts_raw_contacts_into_runtime_contacts() {
        let touch = TouchInput {
            sequence: 5,
            contacts: vec![RawTouchContact {
                id: 9,
                phase: TouchPhase::Started,
                position: ScreenPoint::new(12.0, 34.0),
                pressure: Some(0.7),
            }],
        };

        let frame = touch.into_runtime_frame().expect("valid touch input");
        assert_eq!(frame.sequence, 5);
        assert_eq!(frame.contacts.len(), 1);
        assert_eq!(frame.contacts[0].id, 9);
    }
}
