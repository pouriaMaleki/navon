use std::time::Duration;

use runtime_core::api::{RuntimeInputFrame, TouchContactFrameError, ViewportSize};

use crate::gps::GpsInput;
use crate::touch::TouchInput;

#[derive(Debug, Clone)]
pub struct InputBridge {
    viewport_size: ViewportSize,
}

impl InputBridge {
    pub fn new(viewport_size: ViewportSize) -> Self {
        Self { viewport_size }
    }

    pub fn frame_from_samples(
        &self,
        dt: Duration,
        gps: Option<GpsInput>,
        touch: Option<TouchInput>,
    ) -> Result<RuntimeInputFrame, TouchContactFrameError> {
        let frame = RuntimeInputFrame::new(dt).with_viewport(self.viewport_size);
        let frame = if let Some(gps) = gps {
            frame.with_gps(gps.into())
        } else {
            frame
        };

        touch
            .map(TouchInput::into_runtime_frame)
            .transpose()
            .map(|touch| match touch {
                Some(touch) => frame.with_touch(touch),
                None => frame,
            })
    }
}

impl Default for InputBridge {
    fn default() -> Self {
        Self::new(ViewportSize::new(480, 480))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::touch::{RawTouchContact, TouchInput};
    use runtime_core::api::{ScreenPoint, TouchPhase};

    #[test]
    fn bridge_builds_runtime_input_frame() {
        let bridge = InputBridge::new(ViewportSize::new(320, 240));
        let frame = bridge
            .frame_from_samples(
                Duration::from_millis(16),
                Some(GpsInput {
                    lat_deg: 1.0,
                    lon_deg: 2.0,
                    speed_mps: 3.0,
                    course_rad: Some(0.25),
                    horizontal_accuracy_m: Some(4.0),
                }),
                Some(TouchInput {
                    sequence: 2,
                    contacts: vec![RawTouchContact {
                        id: 1,
                        phase: TouchPhase::Started,
                        position: ScreenPoint::new(20.0, 30.0),
                        pressure: Some(0.5),
                    }],
                }),
            )
            .expect("runtime frame");

        assert_eq!(frame.viewport_size, Some(ViewportSize::new(320, 240)));
        assert!(frame.gps.is_some());
        assert_eq!(frame.touch.as_ref().map(|touch| touch.sequence), Some(2));
    }
}
