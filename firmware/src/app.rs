use std::time::{Duration, Instant};

use crate::framebuffer::RenderFramebuffer;
use runtime_core::RuntimeCore;
use runtime_core::api::{
    GestureEventKind, MapQueryResult, RouteStatusMessage, RuntimeConfig, RuntimeFrameOutput,
    TouchContactFrameError,
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
use crate::world_buffer::{WorldBuffer, render_ui_overlay};

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
    /// Monotonic accumulator of `dt` deltas, used as the "now" handed
    /// to `route_sync_transport.tick(...)` each frame for idle-timeout
    /// bookkeeping. Drift across `Instant::now()` discontinuities is
    /// fine for this purpose since it's only used for relative gaps.
    monotonic_clock: Duration,
    world_buffer: WorldBuffer,
    world_buffer_enabled: bool,
    /// Per-frame render-path tag. Lets tests assert that they actually
    /// exercised the world-buffer cached-blit code path; production
    /// callers ignore it.
    #[cfg(test)]
    last_render_path: FramePath,
}

#[cfg(test)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FramePath {
    /// Cached basemap blit + screen-fixed overlay; `render_world`
    /// skipped. Hot path on idle frames.
    CachedBlit,
    /// World-buffer rebuild (full render into cache) + blit + overlay.
    /// Runs on first frame and whenever the camera leaves the cache.
    RebuildAndBlit,
    /// Direct full render into the display framebuffer; no world buffer.
    /// Runs during rotate/pinch gestures.
    DirectRender,
    /// Initial sentinel before the first frame has run.
    None,
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
            monotonic_clock: Duration::ZERO,
            world_buffer: WorldBuffer::new(),
            world_buffer_enabled: true,
            #[cfg(test)]
            last_render_path: FramePath::None,
        })
    }

    pub fn step_frame(
        &mut self,
        dt: Duration,
        gps: Option<GpsInput>,
        touch: Option<TouchInput>,
    ) -> Result<FrameResult, AppError> {
        // Advance the monotonic clock and let the route-sync transport
        // drop any in-flight transfer that has gone idle. The returned
        // statuses (currently at most one `RetryableFailure` per timeout)
        // surface in `FrameResult.route_sync_statuses` so the platform
        // forwards them back to the companion.
        self.monotonic_clock = self.monotonic_clock.saturating_add(dt);
        let mut transport_tick_statuses = self
            .route_sync_transport
            .tick(self.monotonic_clock);
        let pending_route_sync = self.route_sync_transport.take_pending_runtime_message();
        let input = self.input_bridge.frame_from_samples(dt, gps, touch)?;
        let input = match pending_route_sync {
            Some(route_sync) => input.with_route_sync(route_sync),
            None => input,
        };
        let output = self.runtime.step(input);

        let active_gesture = output.active_gesture;
        let use_world_buffer =
            self.world_buffer_enabled && self.world_buffer.is_valid_for(&output.camera);

        let t_query_start = Instant::now();
        let world_buffer_will_handle =
            self.world_buffer_enabled && active_gesture.is_none() && !use_world_buffer;
        let (geometry, mut geometry_count) = if use_world_buffer || world_buffer_will_handle {
            // Pan/idle: query handled by world buffer path below.
            (MapQueryResult::default(), 0)
        } else {
            // Rotate or pinch: normal query with coarse LOD (already applied by ECS).
            let geo = self.map_source.query(&output.map_query);
            let count = geo.geometry.len();
            (geo, count)
        };
        let map_query = t_query_start.elapsed();

        let t_render_start = Instant::now();
        if use_world_buffer {
            // Fast path: blit pre-rendered basemap, then draw screen-fixed UI.
            geometry_count = self.world_buffer.last_geometry_count();
            self.world_buffer.blit_into(&output.camera, &mut self.render_framebuffer);
            render_ui_overlay(
                self.runtime.config(),
                &output,
                &geometry,
                &mut self.render_framebuffer,
            );
            #[cfg(test)]
            {
                self.last_render_path = FramePath::CachedBlit;
            }
        } else if matches!(
            active_gesture,
            Some(GestureEventKind::Rotate) | Some(GestureEventKind::Pinch)
        ) {
            // Rotate/pinch: full render at coarse LOD; world buffer is now stale.
            self.world_buffer.invalidate();
            render_core::render_frame(
                render_core::RenderScene {
                    config: self.runtime.config(),
                    output: &output,
                    geometry: &geometry,
                },
                &mut self.render_framebuffer,
            );
            #[cfg(test)]
            {
                self.last_render_path = FramePath::DirectRender;
            }
        } else if world_buffer_will_handle {
            // Idle (world buffer enabled but stale): rebuild then blit, UI on top.
            geometry_count = self.world_buffer.render_and_blit(
                &mut self.map_source,
                self.runtime.config(),
                &output,
                &mut self.render_framebuffer,
            );
            render_ui_overlay(
                self.runtime.config(),
                &output,
                &MapQueryResult::default(),
                &mut self.render_framebuffer,
            );
            #[cfg(test)]
            {
                self.last_render_path = FramePath::RebuildAndBlit;
            }
        } else {
            // World buffer disabled, or pan past world-buffer edge with buffer disabled:
            // direct render as before.
            render_core::render_frame(
                render_core::RenderScene {
                    config: self.runtime.config(),
                    output: &output,
                    geometry: &geometry,
                },
                &mut self.render_framebuffer,
            );
            #[cfg(test)]
            {
                self.last_render_path = FramePath::DirectRender;
            }
        }
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
            ..self.persisted_settings
        };
        if next_settings != self.persisted_settings {
            self.settings_store.save_settings(&next_settings)?;
            self.persisted_settings = next_settings;
        }

        // Compose statuses in arrival order: idle-timeout drops fire
        // first (so the companion can react before we surface a new
        // Active for the freshly-applied transfer), then the
        // apply-completion `Active` status (if any).
        let mut route_sync_statuses = std::mem::take(&mut transport_tick_statuses);
        route_sync_statuses.extend(self.route_sync_transport.complete_applied_message());

        Ok(FrameResult {
            output,
            geometry_count,
            lit_pixel_count: self.display.framebuffer().lit_pixel_count(),
            route_sync_statuses,
            phase_timings,
        })
    }

    pub fn display(&self) -> &Display<B> {
        &self.display
    }

    /// Permanently disables the world buffer for this `App` instance. Intended
    /// for parity tests that compare pixel-level output between firmware and
    /// wasm render paths — the world buffer clips segments at 1600×1600 bounds
    /// while the direct render uses 800×800, producing ±1 px Bresenham
    /// differences in diagonal strokes. Tests that check `pixel_hash` should
    /// call this after construction.
    pub fn disable_world_buffer(&mut self) {
        self.world_buffer_enabled = false;
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

    #[cfg(test)]
    pub fn last_render_path(&self) -> FramePath {
        self.last_render_path
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
            ..DeviceSettings::default()
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
                speed_unit: SpeedUnit::Kph,
                ..DeviceSettings::default()
            })
        );
    }

    /// Regression test for the world-buffer fast path silently dropping
    /// the route polyline.
    ///
    /// On idle frames the firmware blits a cached basemap from the
    /// `WorldBuffer` and only runs `render_ui` afterwards — `render_world`
    /// is skipped to save the per-frame query + raster cost. If the
    /// route polyline is rendered only inside `render_world`, it is
    /// missing from every cached-blit frame; the route appears applied
    /// in the runtime state and the BLE companion's status flips to
    /// `Active`, but the rider sees no line on screen.
    ///
    /// This test reproduces the scenario from the production build: it
    /// keeps the world buffer enabled, ingests a route through the same
    /// chunked path the BLE adapter uses, and asserts that route-line
    /// pixels survive into the framebuffer. The previous render-core
    /// route tests only call `render_frame` (which always runs both
    /// `render_world` and `render_ui`), so they passed both before and
    /// after the bug — they could not see the firmware's compositing
    /// gap because `WorldBuffer` lives outside the render-core crate.
    #[test]
    fn route_renders_through_world_buffer_cached_blit_path() {
        // GPS fix and route geometry both land in central Helsinki so
        // the polyline projects into the 800×800 viewport at the
        // default zoom; using farther-apart coordinates makes the route
        // off-screen and the test inconclusive.
        let gps_lon = 24.94310_f64;
        let gps_lat = 60.17442_f64;
        let route = RoutePackage {
            version: RoutePackageVersion::new(1, 0),
            route_id: "world-buffer-route-test".to_owned(),
            revision: 1,
            geometry: vec![
                GeoPoint::new(gps_lat - 0.00040, gps_lon - 0.00060),
                GeoPoint::new(gps_lat, gps_lon),
                GeoPoint::new(gps_lat + 0.00040, gps_lon + 0.00060),
            ],
            maneuvers: vec![
                RouteManeuver {
                    id: "depart".to_owned(),
                    maneuver_type: RouteManeuverType::Depart,
                    location: GeoPoint::new(gps_lat - 0.00040, gps_lon - 0.00060),
                    distance_from_start_m: 0.0,
                    distance_to_next_m: Some(120.0),
                    instruction_text: Some("Depart".to_owned()),
                },
                RouteManeuver {
                    id: "arrive".to_owned(),
                    maneuver_type: RouteManeuverType::Arrive,
                    location: GeoPoint::new(gps_lat + 0.00040, gps_lon + 0.00060),
                    distance_from_start_m: 240.0,
                    distance_to_next_m: None,
                    instruction_text: Some("Arrive".to_owned()),
                },
            ],
            summary: RouteSummary {
                total_distance_m: 240.0,
                estimated_duration_s: 90,
                start_label: Some("Start".to_owned()),
                destination_label: Some("End".to_owned()),
            },
            provenance: RouteProvenance {
                provider: RouteProvider::HslDigitransit,
                source_ref: Some("test:world-buffer".to_owned()),
                generated_at_unix_ms: 1,
            },
        };
        let message = RouteSyncMessage::Set(RouteSetMessage { route });

        // Production-like build: world buffer enabled (matches `App::default()`,
        // which is what the firmware constructs at boot). Calling
        // `disable_world_buffer()` here would mask the very bug we are
        // guarding against.
        let mut app = App::default();

        // Warm-up frames: feed the same GPS fix repeatedly so the
        // camera mode + orientation + zoom stabilise and the world
        // buffer rebuilds against the final camera state. Without this
        // ramp the second frame still runs the rebuild path
        // (`world_buffer_will_handle`) and `render_world` re-draws the
        // route into the cache — masking the very bug we are testing
        // against. Five frames is comfortably past the camera-mode
        // settle window.
        for _ in 0..5 {
            app.step_frame(
                Duration::from_millis(16),
                Some(helsinki_fix(gps_lon, 0.0)),
                None,
            )
            .expect("warm-up frame primes the world buffer");
        }

        // Baseline: take a snapshot of the cached-blit frame BEFORE
        // applying the route. Several palette colors are shared between
        // the route polyline and other UI elements (the rider marker
        // fill is `COLOR_ACCENT_HIGHLIGHT`, which is also
        // `remaining_route_line.color`), so a naive pixel-color scan
        // would match the rider marker even with the route absent.
        // Capturing a baseline lets us assert that *adding the route*
        // raised the route-color pixel count, which is the signal we
        // actually care about.
        assert_eq!(
            app.last_render_path(),
            FramePath::CachedBlit,
            "warm-up should leave the camera stable on the cached-blit path; got {:?}",
            app.last_render_path()
        );
        let style = render_core::style::RenderStyle::default();
        let line = style.remaining_route_line.color;
        let baseline_pixels = app
            .display()
            .framebuffer()
            .pixels()
            .chunks_exact(4)
            .filter(|rgba| rgba[0] == line.r && rgba[1] == line.g && rgba[2] == line.b)
            .count();

        // Push the route through the same chunked path the BLE adapter
        // uses, so we exercise reassembly + apply, not just the
        // in-memory shortcut.
        for chunk in chunk_sync_message(&message, "test-transfer", 64) {
            app.ingest_route_sync_chunk(chunk)
                .expect("chunk accepted");
        }

        // Apply frame: `take_pending_runtime_message` returns the
        // assembled `Set` message, runtime applies it. With the camera
        // already stabilised the world buffer is valid for this frame,
        // so the app takes the cached-blit path (`use_world_buffer ==
        // true`) — exactly the path that previously dropped the route
        // polyline because `render_world` was skipped and
        // `render_route_overlay` was only wired into `render_world`.
        let applied = app
            .step_frame(
                Duration::from_millis(16),
                Some(helsinki_fix(gps_lon, 0.0)),
                None,
            )
            .expect("apply frame composes the route over a cached blit");
        assert_eq!(
            applied.output.route.route_id.as_deref(),
            Some("world-buffer-route-test"),
            "runtime should have the route applied before pixel scan",
        );
        assert_eq!(
            applied.route_sync_statuses.last().map(|s| s.status),
            Some(runtime_core::api::RouteSyncStatusCode::Active),
            "platform status should report `Active` once the runtime has the route",
        );

        // Belt-and-suspenders: one more idle frame at the same camera.
        // This is unambiguously the cached-blit path — the route was
        // already in the runtime state when the previous frame's blit
        // ran, so if anything in the chain (apply ordering, world
        // buffer invalidation policy) changes, this frame still covers
        // the production "rider holds steady on a known route" case.
        app.step_frame(
            Duration::from_millis(16),
            Some(helsinki_fix(gps_lon, 0.0)),
            None,
        )
        .expect("idle frame uses cached blit");

        // Verify we actually exercised the cached-blit path. If the
        // camera kept invalidating the cache, the test would silently
        // fall through to the rebuild-and-blit path — which calls
        // `render_world` and would mask the bug. The assertion fails
        // loudly if the warm-up loop above ever stops settling the
        // camera (e.g. because the runtime's mode-detection or
        // mpp/zoom defaults change).
        assert_eq!(
            app.last_render_path(),
            FramePath::CachedBlit,
            "test must run on the cached-blit path; got {:?}. \
             The warm-up loop above is meant to settle the camera so \
             `is_valid_for` returns true on this frame; if the runtime \
             keeps invalidating the world buffer, lengthen the warm-up.",
            app.last_render_path()
        );

        // Scan the displayed framebuffer for the remaining-route
        // polyline color and compare to the baseline. A delta of just a
        // handful of pixels could be Bresenham noise; the route covers
        // ≥240m of geometry at this zoom so a healthy delta is in the
        // hundreds. Require at least 50 extra route-color pixels so the
        // assertion is robust to small future style/zoom tweaks but
        // still fails decisively when the polyline is missing entirely
        // (the broken state's delta is 0).
        let pixels = app.display().framebuffer().pixels();
        let route_pixels = pixels
            .chunks_exact(4)
            .filter(|rgba| rgba[0] == line.r && rgba[1] == line.g && rgba[2] == line.b)
            .count();
        let added = route_pixels.saturating_sub(baseline_pixels);
        assert!(
            added >= 50,
            "expected the remaining-route polyline (#{:02x}{:02x}{:02x}) to add \
             >=50 pixels over the baseline rider marker; got {added} extra \
             (baseline={baseline_pixels}, with route={route_pixels}). The \
             route polyline must be drawn from `render_ui` (not only from \
             `render_world`) so the world-buffer fast path still composites \
             it onto the cached basemap.",
            line.r,
            line.g,
            line.b,
        );
    }
}
