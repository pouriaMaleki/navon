use std::time::Duration;

use render_core::raster::Framebuffer as RenderFramebuffer;
use runtime_core::RuntimeCore;
use runtime_core::api::{RuntimeConfig, RuntimeFrameOutput, TouchContactFrameError};
use runtime_core::map::MapSource;

use crate::board_config::BoardConfig;
use crate::display::Display;
use crate::gps::GpsInput;
use crate::input_bridge::InputBridge;
use crate::map_source::MapSourceBridge;
use crate::touch::TouchInput;

#[derive(Debug, Clone)]
pub struct FrameResult {
    pub output: RuntimeFrameOutput,
    pub geometry_count: usize,
    pub lit_pixel_count: usize,
}

pub struct App {
    runtime: RuntimeCore,
    input_bridge: InputBridge,
    map_source: MapSourceBridge,
    render_framebuffer: RenderFramebuffer,
    display: Display,
}

impl App {
    pub fn new(board: BoardConfig, config: RuntimeConfig) -> Self {
        Self {
            runtime: RuntimeCore::new(config),
            input_bridge: InputBridge::new(board.viewport_size),
            map_source: MapSourceBridge::default(),
            render_framebuffer: RenderFramebuffer::new(
                board.viewport_size.width_px,
                board.viewport_size.height_px,
            ),
            display: Display::new(),
        }
    }

    pub fn step_frame(
        &mut self,
        dt: Duration,
        gps: Option<GpsInput>,
        touch: Option<TouchInput>,
    ) -> Result<FrameResult, TouchContactFrameError> {
        let input = self.input_bridge.frame_from_samples(dt, gps, touch)?;
        let output = self.runtime.step(input);
        let geometry = self.map_source.query(&output.map_query);
        render_core::render_frame(
            render_core::RenderScene {
                config: self.runtime.config(),
                output: &output,
                geometry: &geometry,
            },
            &mut self.render_framebuffer,
        );
        self.display.present(&self.render_framebuffer);

        Ok(FrameResult {
            output,
            geometry_count: geometry.geometry.len(),
            lit_pixel_count: self.display.framebuffer().lit_pixel_count(),
        })
    }

    pub fn display(&self) -> &Display {
        &self.display
    }
}

impl Default for App {
    fn default() -> Self {
        let board = BoardConfig::default();
        Self::new(
            board,
            RuntimeConfig {
                viewport_size: board.viewport_size,
                ..RuntimeConfig::default()
            },
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use runtime_core::api::{CameraMode, ScreenPoint, TouchPhase};

    use crate::touch::RawTouchContact;

    fn helsinki_fix(lon_deg: f64, speed_mps: f32) -> GpsInput {
        GpsInput {
            lat_deg: 60.17442,
            lon_deg,
            speed_mps,
            course_rad: Some(0.0),
            horizontal_accuracy_m: Some(4.0),
        }
    }

    #[test]
    fn app_runs_shared_runtime_query_render_pipeline() {
        let mut app = App::default();
        app.step_frame(
            Duration::from_millis(16),
            Some(helsinki_fix(24.94210, 0.0)),
            None,
        )
        .expect("initial frame");
        let frame = app
            .step_frame(
                Duration::from_millis(16),
                Some(helsinki_fix(24.94310, 5.0)),
                None,
            )
            .expect("moving frame");

        assert_eq!(frame.output.camera.mode, CameraMode::Riding);
        assert!(frame.geometry_count > 0);
        assert!(frame.lit_pixel_count > 0);
        assert_eq!(app.display().presented_frames(), 2);
        assert_eq!(app.display().framebuffer().width(), 800);
        assert_eq!(app.display().framebuffer().height(), 800);
        assert!(!app.display().framebuffer().pixels().is_empty());
    }

    #[test]
    fn app_forwards_touch_through_shared_runtime() {
        let mut app = App::default();
        app.step_frame(
            Duration::from_millis(16),
            Some(helsinki_fix(24.94210, 0.0)),
            None,
        )
        .expect("initial frame");
        app.step_frame(
            Duration::from_millis(16),
            Some(helsinki_fix(24.94310, 5.0)),
            None,
        )
        .expect("moving frame");
        app.step_frame(
            Duration::from_millis(16),
            Some(helsinki_fix(24.94310, 5.0)),
            Some(TouchInput {
                sequence: 1,
                contacts: vec![RawTouchContact {
                    id: 1,
                    phase: TouchPhase::Started,
                    position: ScreenPoint::new(120.0, 120.0),
                    pressure: Some(0.5),
                }],
            }),
        )
        .expect("touch start");
        app.step_frame(
            Duration::from_millis(16),
            Some(helsinki_fix(24.94310, 5.0)),
            Some(TouchInput {
                sequence: 2,
                contacts: vec![RawTouchContact {
                    id: 1,
                    phase: TouchPhase::Moved,
                    position: ScreenPoint::new(160.0, 120.0),
                    pressure: Some(0.5),
                }],
            }),
        )
        .expect("touch move");
        let dragged = app
            .step_frame(
                Duration::from_millis(16),
                Some(helsinki_fix(24.94310, 5.0)),
                Some(TouchInput {
                    sequence: 3,
                    contacts: vec![RawTouchContact {
                        id: 1,
                        phase: TouchPhase::Moved,
                        position: ScreenPoint::new(210.0, 120.0),
                        pressure: Some(0.5),
                    }],
                }),
            )
            .expect("touch drag");

        assert!(dragged.output.camera.follow_locked);
    }
}
