use std::time::{Duration, Instant};

use render_core::raster::Framebuffer as RenderFramebuffer;
use runtime_core::map::MapSource;

use crate::app::{App, AppError, FrameResult};
use crate::display::DisplayBackend;
use crate::gps::{GpsError, GpsInput, GpsProvider};
use crate::touch::{TouchError, TouchInput, TouchSource};

pub trait FrameClock {
    fn next_dt(&mut self) -> Duration;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FixedFrameClock {
    dt: Duration,
}

impl FixedFrameClock {
    pub const fn new(dt: Duration) -> Self {
        Self { dt }
    }
}

impl FrameClock for FixedFrameClock {
    fn next_dt(&mut self) -> Duration {
        self.dt
    }
}

#[derive(Debug, Clone)]
pub struct SystemFrameClock {
    fallback_dt: Duration,
    previous_tick: Option<Instant>,
}

impl SystemFrameClock {
    pub fn new(fallback_dt: Duration) -> Self {
        Self {
            fallback_dt,
            previous_tick: None,
        }
    }
}

impl FrameClock for SystemFrameClock {
    fn next_dt(&mut self) -> Duration {
        let now = Instant::now();
        let dt = self
            .previous_tick
            .map(|previous| now.saturating_duration_since(previous))
            .unwrap_or(self.fallback_dt);
        self.previous_tick = Some(now);
        if dt.is_zero() { self.fallback_dt } else { dt }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PlatformError {
    Gps(GpsError),
    Touch(TouchError),
    App(AppError),
}

impl From<GpsError> for PlatformError {
    fn from(value: GpsError) -> Self {
        Self::Gps(value)
    }
}

impl From<TouchError> for PlatformError {
    fn from(value: TouchError) -> Self {
        Self::Touch(value)
    }
}

impl From<AppError> for PlatformError {
    fn from(value: AppError) -> Self {
        Self::App(value)
    }
}

pub struct RuntimePlatform<
    T,
    G,
    C,
    S = crate::map_source::MapSourceBridge,
    B = crate::display::MemoryDisplayBackend,
> where
    T: TouchSource,
    G: GpsProvider,
    C: FrameClock,
    S: MapSource,
    B: DisplayBackend,
{
    app: App<S, B>,
    touch: T,
    gps: G,
    clock: C,
}

impl<T, G, C, S, B> RuntimePlatform<T, G, C, S, B>
where
    T: TouchSource,
    G: GpsProvider,
    C: FrameClock,
    S: MapSource,
    B: DisplayBackend,
{
    pub fn new(app: App<S, B>, touch: T, gps: G, clock: C) -> Self {
        Self {
            app,
            touch,
            gps,
            clock,
        }
    }

    pub fn run_frame(&mut self) -> Result<FrameResult, PlatformError> {
        let dt = self.clock.next_dt();
        let gps = self.gps.poll()?;
        let touch = self.touch.poll()?;
        Ok(self.app.step_frame(dt, gps, touch)?)
    }

    pub fn app(&self) -> &App<S, B> {
        &self.app
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct NullTouchSource;

impl TouchSource for NullTouchSource {
    fn poll(&mut self) -> Result<Option<TouchInput>, TouchError> {
        Ok(None)
    }
}

pub fn run_host_demo() -> Result<FrameResult, PlatformError> {
    let board = crate::board_config::BoardConfig::default();
    let app = App::new(
        board,
        runtime_core::api::RuntimeConfig {
            viewport_size: board.viewport_size,
            ..runtime_core::api::RuntimeConfig::default()
        },
    );
    let gps = crate::gps::SequenceGpsProvider::new([
        Some(GpsInput {
            lat_deg: 60.17442,
            lon_deg: 24.94210,
            speed_mps: 0.0,
            course_rad: Some(0.0),
            horizontal_accuracy_m: Some(4.0),
        }),
        Some(GpsInput {
            lat_deg: 60.17442,
            lon_deg: 24.94310,
            speed_mps: 5.0,
            course_rad: Some(0.0),
            horizontal_accuracy_m: Some(4.0),
        }),
    ]);
    let mut platform = RuntimePlatform::new(
        app,
        NullTouchSource,
        gps,
        FixedFrameClock::new(board.frame_interval),
    );

    let _ = platform.run_frame()?;
    platform.run_frame()
}

pub fn render_preview_checksum(framebuffer: &RenderFramebuffer) -> u64 {
    framebuffer.pixels().iter().fold(0_u64, |checksum, value| {
        checksum.wrapping_mul(16777619) ^ u64::from(*value)
    })
}

#[cfg(test)]
mod tests {
    use std::collections::VecDeque;

    use runtime_core::api::{ScreenPoint, TouchPhase};

    use super::*;
    use crate::gps::NullGpsProvider;

    #[derive(Debug, Default)]
    struct SequenceTouchSource {
        frames: VecDeque<Option<TouchInput>>,
    }

    impl SequenceTouchSource {
        fn new(frames: impl IntoIterator<Item = Option<TouchInput>>) -> Self {
            Self {
                frames: frames.into_iter().collect(),
            }
        }
    }

    impl TouchSource for SequenceTouchSource {
        fn poll(&mut self) -> Result<Option<TouchInput>, TouchError> {
            Ok(self.frames.pop_front().flatten())
        }
    }

    #[test]
    fn runtime_platform_runs_touch_only_frame_without_gps() {
        let board = crate::board_config::BoardConfig::default();
        let app = App::default();
        let touch = SequenceTouchSource::new([Some(TouchInput {
            sequence: 1,
            contacts: vec![crate::touch::RawTouchContact {
                id: 1,
                phase: TouchPhase::Started,
                position: ScreenPoint::new(100.0, 100.0),
                pressure: Some(0.5),
            }],
        })]);
        let mut platform = RuntimePlatform::new(
            app,
            touch,
            NullGpsProvider,
            FixedFrameClock::new(board.frame_interval),
        );

        let frame = platform.run_frame().expect("platform frame");

        assert_eq!(frame.output.frame_index, 1);
        assert!(platform.app().display().presented_frames() >= 1);
    }
}
