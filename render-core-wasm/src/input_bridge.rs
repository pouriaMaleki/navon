use std::time::Duration;

use runtime_core::api::{
    GpsSample, RuntimeInputFrame, ScreenPoint, TouchContact, TouchContactFrame,
    TouchContactFrameError, TouchPhase, ViewportSize,
};

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BrowserTouchContact {
    pub id: u64,
    pub phase: TouchPhase,
    pub x_px: f32,
    pub y_px: f32,
    pub pressure: Option<f32>,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct InputBridge;

impl InputBridge {
    pub fn frame_from_browser(
        &self,
        dt: Duration,
        viewport_size: ViewportSize,
        gps: Option<GpsSample>,
        sequence: u64,
        contacts: Vec<BrowserTouchContact>,
    ) -> Result<RuntimeInputFrame, TouchContactFrameError> {
        let touch = TouchContactFrame::new(
            sequence,
            contacts
                .into_iter()
                .map(|contact| TouchContact {
                    id: contact.id,
                    phase: contact.phase,
                    position: ScreenPoint::new(contact.x_px, contact.y_px),
                    pressure: contact.pressure,
                })
                .collect(),
        )?;

        let frame = RuntimeInputFrame::new(dt).with_viewport(viewport_size);
        let frame = if let Some(gps) = gps {
            frame.with_gps(gps)
        } else {
            frame
        };
        Ok(frame.with_touch(touch))
    }
}
