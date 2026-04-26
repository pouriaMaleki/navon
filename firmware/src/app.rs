use std::time::{Duration, Instant};

use crate::framebuffer::RenderFramebuffer;
use runtime_core::RuntimeCore;
use runtime_core::api::{
    RouteStatusMessage, RuntimeConfig, RuntimeFrameOutput, TouchContactFrameError,
};
use runtime_core::map::MapSource;

use crate::board_config::BoardConfig;
use crate::display::{Display, DisplayBackend, DisplayError, MemoryDisplayBackend};
use crate::gps::GpsInput;
use crate::input_bridge::InputBridge;
use crate::map_source::MapSourceBridge;
use crate::route_sync::{RouteSyncTransport, RouteSyncTransportError, RouteTransferChunk};
use crate::settings::{DeviceSettings, NullSettingsStore, SettingsError, SettingsStore};
use crate::touch::TouchInput;

/// Per-phase timings for one `step_frame` call. All zero on host builds
/// where timing isn't recorded; on device this lets the boot loop split
/// the 186 ms baseline into "where does the time actually go".
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct PhaseTimings {
    pub map_query: Duration,
    pub render: Duration,
    pub convert: Duration,
    pub panel_push: Duration,
}

#[derive(Debug, Clone)]
pub struct FrameResult {
    pub output: RuntimeFrameOutput,
    pub geometry_count: usize,
    pub lit_pixel_count: usize,
    pub route_sync_statuses: Vec<RouteStatusMessage>,
    pub phase_timings: PhaseTimings,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AppError {
    InvalidTouchFrame(TouchContactFrameError),
    Display(DisplayError),
    Settings(SettingsError),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AppBuildError {
    Display(DisplayError),
    Settings(SettingsError),
}

impl From<TouchContactFrameError> for AppError {
    fn from(value: TouchContactFrameError) -> Self {
        Self::InvalidTouchFrame(value)
    }
}

impl From<DisplayError> for AppError {
    fn from(value: DisplayError) -> Self {
        Self::Display(value)
    }
}

impl From<SettingsError> for AppError {
    fn from(value: SettingsError) -> Self {
        Self::Settings(value)
    }
}

impl From<DisplayError> for AppBuildError {
    fn from(value: DisplayError) -> Self {
        Self::Display(value)
    }
}

impl From<SettingsError> for AppBuildError {
    fn from(value: SettingsError) -> Self {
        Self::Settings(value)
    }
}

pub struct App<S = MapSourceBridge, B = MemoryDisplayBackend, U = NullSettingsStore>
where
    S: MapSource,
    B: DisplayBackend,
    U: SettingsStore,
{
    runtime: RuntimeCore,
    input_bridge: InputBridge,
    map_source: S,
    render_framebuffer: RenderFramebuffer,
    display: Display<B>,
    settings_store: U,
    persisted_settings: DeviceSettings,
    route_sync_transport: RouteSyncTransport,
}

impl<S, B, U> App<S, B, U>
where
    S: MapSource,
    B: DisplayBackend,
    U: SettingsStore,
{
    pub fn with_parts_and_settings(
        board: BoardConfig,
        mut config: RuntimeConfig,
        map_source: S,
        display_backend: B,
        mut settings_store: U,
    ) -> Result<Self, AppBuildError> {
        let persisted_settings = settings_store.load_settings()?.unwrap_or_default();
        config.default_speed_unit = persisted_settings.speed_unit;
        Ok(Self {
            runtime: RuntimeCore::new(config),
            input_bridge: InputBridge::new(board.viewport_size),
            map_source,
            render_framebuffer: RenderFramebuffer::new(
                board.viewport_size.width_px,
                board.viewport_size.height_px,
            ),
            display: Display::with_backend(board.display, display_backend)?,
            settings_store,
            persisted_settings,
            route_sync_transport: RouteSyncTransport::default(),
        })
    }

    pub fn step_frame(
        &mut self,
        dt: Duration,
        gps: Option<GpsInput>,
        touch: Option<TouchInput>,
    ) -> Result<FrameResult, AppError> {
        let pending_route_sync = self.route_sync_transport.take_pending_runtime_message();
        let input = self.input_bridge.frame_from_samples(dt, gps, touch)?;
        let input = match pending_route_sync {
            Some(route_sync) => input.with_route_sync(route_sync),
            None => input,
        };
        let output = self.runtime.step(input);

        let t_query_start = Instant::now();
        let geometry = self.map_source.query(&output.map_query);
        let map_query = t_query_start.elapsed();

        let t_render_start = Instant::now();
        render_core::render_frame(
            render_core::RenderScene {
                config: self.runtime.config(),
                output: &output,
                geometry: &geometry,
            },
            &mut self.render_framebuffer,
        );
        let render = t_render_start.elapsed();

        let (convert, panel_push) = self.display.present_timed(&self.render_framebuffer)?;
        let phase_timings = PhaseTimings {
            map_query,
            render,
            convert,
            panel_push,
        };

        let next_settings = DeviceSettings {
            speed_unit: output.overlay.speed_unit,
        };
        if next_settings != self.persisted_settings {
            self.settings_store.save_settings(&next_settings)?;
            self.persisted_settings = next_settings;
        }

        let route_sync_statuses = self
            .route_sync_transport
            .complete_applied_message()
            .into_iter()
            .collect();

        Ok(FrameResult {
            output,
            geometry_count: geometry.geometry.len(),
            lit_pixel_count: self.display.framebuffer().lit_pixel_count(),
            route_sync_statuses,
            phase_timings,
        })
    }

    pub fn display(&self) -> &Display<B> {
        &self.display
    }

    pub fn ingest_route_sync_chunk(
        &mut self,
        chunk: RouteTransferChunk,
    ) -> Result<Vec<RouteStatusMessage>, RouteSyncTransportError> {
        self.route_sync_transport.ingest_chunk(chunk)
    }

    pub fn route_sync_transport(&self) -> &RouteSyncTransport {
        &self.route_sync_transport
    }
}

impl<S, B> App<S, B, NullSettingsStore>
where
    S: MapSource,
    B: DisplayBackend,
{
    pub fn with_parts(
        board: BoardConfig,
        config: RuntimeConfig,
        map_source: S,
        display_backend: B,
    ) -> Result<Self, AppBuildError> {
        Self::with_parts_and_settings(
            board,
            config,
            map_source,
            display_backend,
            NullSettingsStore,
        )
    }
}

impl<S, U> App<S, MemoryDisplayBackend, U>
where
    S: MapSource,
    U: SettingsStore,
{
    pub fn with_map_source_and_settings(
        board: BoardConfig,
        config: RuntimeConfig,
        map_source: S,
        speed_unit_store: U,
    ) -> Self {
        Self::with_parts_and_settings(
            board,
            config,
            map_source,
            MemoryDisplayBackend::default(),
            speed_unit_store,
        )
        .expect("memory display backend and settings store should initialize")
    }
}

impl<S> App<S, MemoryDisplayBackend, NullSettingsStore>
where
    S: MapSource,
{
    pub fn with_map_source(board: BoardConfig, config: RuntimeConfig, map_source: S) -> Self {
        Self::with_map_source_and_settings(board, config, map_source, NullSettingsStore)
    }
}

impl App<MapSourceBridge, MemoryDisplayBackend, NullSettingsStore> {
    pub fn new(board: BoardConfig, config: RuntimeConfig) -> Self {
        Self::with_map_source(board, config, MapSourceBridge::default())
    }
}

impl Default for App<MapSourceBridge, MemoryDisplayBackend, NullSettingsStore> {
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
    use runtime_core::api::{
        CameraMode, GeoPoint, RouteManeuver, RouteManeuverType, RoutePackage, RoutePackageVersion,
        RouteProvenance, RouteProvider, RouteSetMessage, RouteSummary, RouteSyncMessage,
        ScreenPoint, SpeedUnit, TouchPhase,
    };

    use crate::board_config::BoardConfig;
    use crate::display::MemoryDisplayBackend;
    use crate::route_sync::chunk_sync_message;
    use crate::settings::{DeviceSettings, MemorySettingsStore};
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
    fn app_applies_route_sync_transfer_into_runtime() {
        let mut app = App::default();
        let message = RouteSyncMessage::Set(RouteSetMessage {
            route: sample_route(1),
        });

        let mut statuses = Vec::new();
        for chunk in chunk_sync_message(&message, "transfer-1", 32) {
            statuses.extend(app.ingest_route_sync_chunk(chunk).expect("chunk accepted"));
        }

        assert_eq!(statuses.len(), 2);
        assert_eq!(
            statuses[0].status,
            runtime_core::api::RouteSyncStatusCode::Accepted
        );
        assert_eq!(
            statuses[1].status,
            runtime_core::api::RouteSyncStatusCode::Applying
        );

        let frame = app
            .step_frame(Duration::from_millis(16), None, None)
            .expect("frame with route sync");

        assert_eq!(
            frame.output.route.route_id.as_deref(),
            Some("hsl:kamppi->kallio:alt-0")
        );
        assert_eq!(frame.output.route.revision, Some(1));
        assert_eq!(frame.route_sync_statuses.len(), 1);
        assert_eq!(
            frame.route_sync_statuses[0].status,
            runtime_core::api::RouteSyncStatusCode::Active
        );
        assert_eq!(app.route_sync_transport().active_route_revision(), Some(1));
    }

    #[test]
    fn app_restores_and_persists_speed_unit_preference() {
        let board = BoardConfig::default();
        let store = MemorySettingsStore::new(Some(DeviceSettings {
            speed_unit: SpeedUnit::Mph,
        }));
        let mut app = App::with_parts_and_settings(
            board,
            RuntimeConfig {
                viewport_size: board.viewport_size,
                ..RuntimeConfig::default()
            },
            MapSourceBridge::default(),
            MemoryDisplayBackend::default(),
            store.clone(),
        )
        .expect("app with settings");

        let moving = app
            .step_frame(
                Duration::from_millis(16),
                Some(helsinki_fix(24.94310, 5.0)),
                None,
            )
            .expect("moving frame");

        assert_eq!(moving.output.overlay.speed_unit, SpeedUnit::Mph);

        app.step_frame(
            Duration::from_millis(16),
            Some(helsinki_fix(24.94310, 5.0)),
            Some(TouchInput {
                sequence: 1,
                contacts: vec![RawTouchContact {
                    id: 1,
                    phase: TouchPhase::Started,
                    position: ScreenPoint::new(400.0, 720.0),
                    pressure: Some(0.5),
                }],
            }),
        )
        .expect("speed unit tap start");
        app.step_frame(
            Duration::from_millis(16),
            Some(helsinki_fix(24.94310, 5.0)),
            Some(TouchInput {
                sequence: 2,
                contacts: vec![RawTouchContact {
                    id: 1,
                    phase: TouchPhase::Ended,
                    position: ScreenPoint::new(400.0, 720.0),
                    pressure: Some(0.5),
                }],
            }),
        )
        .expect("speed unit toggle tap");

        assert_eq!(
            store.shared_value(),
            Some(DeviceSettings {
                speed_unit: SpeedUnit::Kph
            })
        );
    }
}
