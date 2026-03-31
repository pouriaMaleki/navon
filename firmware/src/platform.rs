use std::time::{Duration, Instant};

use runtime_core::api::{GeoPoint, RouteRerouteRequestMessage, RouteSyncMessage};
use runtime_core::map::MapSource;

use crate::app::{App, AppError, FrameResult};
use crate::display::DisplayBackend;
use crate::gps::{GpsError, GpsInput, GpsProvider};
use crate::route_sync::{RouteSyncTransportError, RouteTransferChunk};
use crate::settings::{NullSettingsStore, SettingsStore};
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
pub enum RouteSyncIoError {
    Transport(String),
}

pub trait RouteSyncIo {
    fn poll_chunk(&mut self) -> Result<Option<RouteTransferChunk>, RouteSyncIoError>;
    fn publish_messages(&mut self, messages: &[RouteSyncMessage])
    -> Result<(), RouteSyncIoError>;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct NullRouteSyncIo;

impl RouteSyncIo for NullRouteSyncIo {
    fn poll_chunk(&mut self) -> Result<Option<RouteTransferChunk>, RouteSyncIoError> {
        Ok(None)
    }

    fn publish_messages(
        &mut self,
        _messages: &[RouteSyncMessage],
    ) -> Result<(), RouteSyncIoError> {
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum PlatformError {
    Gps(GpsError),
    Touch(TouchError),
    App(AppError),
    RouteSyncTransport(RouteSyncTransportError),
    RouteSyncIo(RouteSyncIoError),
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

impl From<RouteSyncTransportError> for PlatformError {
    fn from(value: RouteSyncTransportError) -> Self {
        Self::RouteSyncTransport(value)
    }
}

impl From<RouteSyncIoError> for PlatformError {
    fn from(value: RouteSyncIoError) -> Self {
        Self::RouteSyncIo(value)
    }
}

pub struct RuntimePlatform<
    T,
    G,
    C,
    S = crate::map_source::MapSourceBridge,
    B = crate::display::MemoryDisplayBackend,
    U = NullSettingsStore,
    R = NullRouteSyncIo,
> where
    T: TouchSource,
    G: GpsProvider,
    C: FrameClock,
    R: RouteSyncIo,
    S: MapSource,
    B: DisplayBackend,
    U: SettingsStore,
{
    app: App<S, B, U>,
    touch: T,
    gps: G,
    clock: C,
    route_sync: R,
    last_reroute_requested: bool,
}

impl<T, G, C, S, B, U, R> RuntimePlatform<T, G, C, S, B, U, R>
where
    T: TouchSource,
    G: GpsProvider,
    C: FrameClock,
    S: MapSource,
    B: DisplayBackend,
    U: SettingsStore,
    R: RouteSyncIo,
{
    pub fn with_route_sync(app: App<S, B, U>, touch: T, gps: G, clock: C, route_sync: R) -> Self {
        Self {
            app,
            touch,
            gps,
            clock,
            route_sync,
            last_reroute_requested: false,
        }
    }

    pub fn run_frame(&mut self) -> Result<FrameResult, PlatformError> {
        let mut route_sync_statuses = Vec::new();
        while let Some(chunk) = self.route_sync.poll_chunk()? {
            match self.app.ingest_route_sync_chunk(chunk) {
                Ok(statuses) => route_sync_statuses.extend(statuses),
                Err(error) => route_sync_statuses.push(error.as_status_message()),
            }
        }

        let dt = self.clock.next_dt();
        let gps = self.gps.poll()?;
        let touch = self.touch.poll()?;
        let mut frame = self.app.step_frame(dt, gps, touch)?;
        route_sync_statuses.extend(frame.route_sync_statuses.iter().cloned());

        let mut outbound_messages = route_sync_statuses
            .iter()
            .cloned()
            .map(RouteSyncMessage::Status)
            .collect::<Vec<_>>();

        if frame.output.route.reroute_requested
            && !self.last_reroute_requested
            && frame.output.route.route_id.is_some()
            && gps.is_some()
        {
            let gps = gps.expect("checked gps availability");
            outbound_messages.push(RouteSyncMessage::RerouteRequest(RouteRerouteRequestMessage {
                route_id: frame.output.route.route_id.clone(),
                rider_position: GeoPoint::new(gps.lat_deg, gps.lon_deg),
                reason: "off_route".to_owned(),
            }));
        }
        self.last_reroute_requested = frame.output.route.reroute_requested;

        if !outbound_messages.is_empty() {
            self.route_sync.publish_messages(&outbound_messages)?;
            frame.route_sync_statuses = route_sync_statuses;
        }

        Ok(frame)
    }

    pub fn app(&self) -> &App<S, B, U> {
        &self.app
    }

    pub fn route_sync(&self) -> &R {
        &self.route_sync
    }
}

impl<T, G, C, S, B, U> RuntimePlatform<T, G, C, S, B, U, NullRouteSyncIo>
where
    T: TouchSource,
    G: GpsProvider,
    C: FrameClock,
    S: MapSource,
    B: DisplayBackend,
    U: SettingsStore,
{
    pub fn new(app: App<S, B, U>, touch: T, gps: G, clock: C) -> Self {
        Self::with_route_sync(app, touch, gps, clock, NullRouteSyncIo)
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

#[cfg(test)]
mod tests {
    use std::collections::VecDeque;

    use runtime_core::api::{
        GeoPoint, RouteManeuver, RouteManeuverType, RoutePackage, RoutePackageVersion,
        RouteProvenance, RouteProvider, RouteSetMessage, RouteSummary, RouteSyncMessage,
        RouteSyncStatusCode, ScreenPoint, TouchPhase,
    };

    use super::*;
    use crate::gps::{NullGpsProvider, SequenceGpsProvider};
    use crate::route_sync::chunk_sync_message;

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

    #[derive(Debug, Default)]
    struct SequenceRouteSyncIo {
        inbound_chunks: VecDeque<RouteTransferChunk>,
        published_batches: Vec<Vec<RouteSyncMessage>>,
    }

    impl SequenceRouteSyncIo {
        fn new(chunks: impl IntoIterator<Item = RouteTransferChunk>) -> Self {
            Self {
                inbound_chunks: chunks.into_iter().collect(),
                published_batches: Vec::new(),
            }
        }

        fn published_messages(&self) -> &[Vec<RouteSyncMessage>] {
            &self.published_batches
        }
    }

    impl RouteSyncIo for SequenceRouteSyncIo {
        fn poll_chunk(&mut self) -> Result<Option<RouteTransferChunk>, RouteSyncIoError> {
            Ok(self.inbound_chunks.pop_front())
        }

        fn publish_messages(
            &mut self,
            messages: &[RouteSyncMessage],
        ) -> Result<(), RouteSyncIoError> {
            self.published_batches.push(messages.to_vec());
            Ok(())
        }
    }

    fn sample_route(revision: u64) -> RoutePackage {
        RoutePackage {
            version: RoutePackageVersion::new(1, 0),
            route_id: "hsl:kamppi->kallio:alt-0".to_owned(),
            revision,
            geometry: vec![
                GeoPoint::new(60.1699, 24.9384),
                GeoPoint::new(60.1712, 24.9443),
            ],
            maneuvers: vec![
                RouteManeuver {
                    id: "depart".to_owned(),
                    maneuver_type: RouteManeuverType::Depart,
                    location: GeoPoint::new(60.1699, 24.9384),
                    distance_from_start_m: 0.0,
                    distance_to_next_m: Some(120.0),
                    instruction_text: Some("Start riding".to_owned()),
                },
                RouteManeuver {
                    id: "arrive".to_owned(),
                    maneuver_type: RouteManeuverType::Arrive,
                    location: GeoPoint::new(60.1712, 24.9443),
                    distance_from_start_m: 120.0,
                    distance_to_next_m: None,
                    instruction_text: Some("Arrive".to_owned()),
                },
            ],
            summary: RouteSummary {
                total_distance_m: 120.0,
                estimated_duration_s: 45,
                start_label: Some("Kamppi".to_owned()),
                destination_label: Some("Kallio".to_owned()),
            },
            provenance: RouteProvenance {
                provider: RouteProvider::HslDigitransit,
                source_ref: Some("digitransit:test".to_owned()),
                generated_at_unix_ms: 1,
            },
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

    #[test]
    fn runtime_platform_forwards_route_sync_between_transport_and_app() {
        let board = crate::board_config::BoardConfig::default();
        let app = App::default();
        let message = RouteSyncMessage::Set(RouteSetMessage {
            route: sample_route(1),
        });
        let route_sync = SequenceRouteSyncIo::new(chunk_sync_message(&message, "transfer-1", 32));
        let mut platform = RuntimePlatform::with_route_sync(
            app,
            NullTouchSource,
            NullGpsProvider,
            FixedFrameClock::new(board.frame_interval),
            route_sync,
        );

        let frame = platform
            .run_frame()
            .expect("platform frame with route sync");

        assert_eq!(
            frame.output.route.route_id.as_deref(),
            Some("hsl:kamppi->kallio:alt-0")
        );
        assert_eq!(frame.output.route.revision, Some(1));
        assert_eq!(frame.route_sync_statuses.len(), 3);
        assert_eq!(
            frame.route_sync_statuses[0].status,
            RouteSyncStatusCode::Accepted
        );
        assert_eq!(
            frame.route_sync_statuses[1].status,
            RouteSyncStatusCode::Applying
        );
        assert_eq!(
            frame.route_sync_statuses[2].status,
            RouteSyncStatusCode::Active
        );
        assert_eq!(platform.route_sync().published_messages().len(), 1);
        assert_eq!(
            platform.route_sync().published_messages()[0],
            frame
                .route_sync_statuses
                .iter()
                .cloned()
                .map(RouteSyncMessage::Status)
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn runtime_platform_reports_retryable_failure_for_checksum_mismatch_without_crashing() {
        let board = crate::board_config::BoardConfig::default();
        let app = App::default();
        let message = RouteSyncMessage::Set(RouteSetMessage {
            route: sample_route(1),
        });
        let mut chunks = chunk_sync_message(&message, "transfer-1", 32);
        chunks[0].checksum_hex = "deadbeef".to_owned();
        let route_sync = SequenceRouteSyncIo::new(chunks);
        let mut platform = RuntimePlatform::with_route_sync(
            app,
            NullTouchSource,
            NullGpsProvider,
            FixedFrameClock::new(board.frame_interval),
            route_sync,
        );

        let frame = platform.run_frame().expect("platform frame should survive checksum mismatch");

        assert_eq!(frame.route_sync_statuses.len(), 1);
        assert_eq!(frame.route_sync_statuses[0].status, RouteSyncStatusCode::RetryableFailure);
        assert_eq!(platform.route_sync().published_messages().len(), 1);
        assert!(matches!(
            platform.route_sync().published_messages()[0].first(),
            Some(RouteSyncMessage::Status(status)) if status.status == RouteSyncStatusCode::RetryableFailure
        ));
    }

    #[test]
    fn runtime_platform_reports_fatal_failure_for_malformed_payload_without_crashing() {
        let board = crate::board_config::BoardConfig::default();
        let app = App::default();
        let payload = b"kind=set
route_id=broken-only".to_vec();
        let chunk = RouteTransferChunk {
            transfer_id: "broken-transfer".to_owned(),
            chunk_index: 0,
            total_chunks: 1,
            checksum_hex: crate::route_sync::checksum_hex(&payload),
            payload_fragment: payload,
        };
        let route_sync = SequenceRouteSyncIo::new([chunk]);
        let mut platform = RuntimePlatform::with_route_sync(
            app,
            NullTouchSource,
            NullGpsProvider,
            FixedFrameClock::new(board.frame_interval),
            route_sync,
        );

        let frame = platform.run_frame().expect("platform frame should survive malformed payload");

        assert_eq!(frame.route_sync_statuses.len(), 1);
        assert_eq!(frame.route_sync_statuses[0].status, RouteSyncStatusCode::FatalFailure);
        assert_eq!(platform.route_sync().published_messages().len(), 1);
        assert!(matches!(
            platform.route_sync().published_messages()[0].first(),
            Some(RouteSyncMessage::Status(status)) if status.status == RouteSyncStatusCode::FatalFailure
        ));
    }

    #[test]
    fn runtime_platform_publishes_reroute_request_when_runtime_flags_off_route_reroute() {
        let app = App::default();
        let message = RouteSyncMessage::Set(RouteSetMessage {
            route: sample_route(1),
        });
        let route_sync = SequenceRouteSyncIo::new(chunk_sync_message(&message, "transfer-1", 32));
        let gps = SequenceGpsProvider::new([
            Some(GpsInput {
                lat_deg: 60.1699,
                lon_deg: 24.9384,
                speed_mps: 4.0,
                course_rad: Some(0.0),
                horizontal_accuracy_m: Some(4.0),
            }),
            Some(GpsInput {
                lat_deg: 60.1799,
                lon_deg: 24.9584,
                speed_mps: 4.0,
                course_rad: Some(0.0),
                horizontal_accuracy_m: Some(4.0),
            }),
        ]);
        let mut platform = RuntimePlatform::with_route_sync(
            app,
            NullTouchSource,
            gps,
            FixedFrameClock::new(Duration::from_secs(3)),
            route_sync,
        );

        let _ = platform.run_frame().expect("activation frame");
        let frame = platform.run_frame().expect("off-route reroute frame");

        assert!(frame.output.route.reroute_requested);
        assert_eq!(platform.route_sync().published_messages().len(), 2);
        assert!(matches!(
            platform.route_sync().published_messages()[1].last(),
            Some(RouteSyncMessage::RerouteRequest(request))
                if request.route_id.as_deref() == Some("hsl:kamppi->kallio:alt-0")
                && request.reason == "off_route"
        ));
    }
}
