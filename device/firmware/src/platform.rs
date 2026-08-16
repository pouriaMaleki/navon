use std::time::{Duration, Instant};

use runtime_core::api::{GeoPoint, RouteRerouteRequestMessage, RouteSyncMessage};
use runtime_core::map::MapSource;

use crate::app::{App, AppError, FrameResult};
use crate::display::DisplayBackend;
use crate::fuel_gauge::FuelGaugeReading;
use crate::gps::{GpsError, GpsInput, GpsProvider, GpsSource};
use crate::route_sync::{RouteSyncTransportError, RouteTransferChunk};
use crate::settings::{NullSettingsStore, SettingsStore};
use crate::touch::{TouchError, TouchInput, TouchSource};

/// How long we tolerate no fresh RMC fix before flipping the
/// "GETTING GPS" overlay back on. Set against the NEO-6M's 1 Hz
/// default emit rate plus a few seconds of slack so a single dropped
/// sentence (multipath, brief obstruction) doesn't blink the overlay.
const GPS_LOST_THRESHOLD_MS: u32 = 10_000;

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
    fn publish_messages(&mut self, messages: &[RouteSyncMessage]) -> Result<(), RouteSyncIoError>;

    /// Whether a `pairing_request` write came in since the last call.
    /// Default no-op so non-BLE transports (`NullRouteSyncIo`, host
    /// fakes) don't have to implement it. The hosted-BLE impl drains
    /// the C-side flag set by the GATT trampoline.
    fn poll_pairing_request(&mut self) -> Result<bool, RouteSyncIoError> {
        Ok(false)
    }

    /// Pull the next queued pairing-confirm secret (32 bytes), or
    /// `None` if no companion has written one since the last call.
    /// Default no-op for the same reason.
    fn poll_pairing_secret(
        &mut self,
    ) -> Result<Option<[u8; crate::pairing::SECRET_LEN]>, RouteSyncIoError> {
        Ok(None)
    }

    /// Pull the next queued SMP auth-completion event, or `None` if
    /// none happened since the last call. Default no-op for non-BLE
    /// transports. The hosted-BLE impl drains the queue populated by
    /// the C-side `ESP_GAP_BLE_AUTH_CMPL_EVT` trampoline.
    fn poll_auth_cmpl(&mut self) -> Result<Option<AuthCmplOutcome>, RouteSyncIoError> {
        Ok(None)
    }

    /// Drain the latest phone GPS sample from the BLE inbound queue.
    /// Returns the raw CSV bytes (`lat,lon,speed,course,accuracy`) or
    /// `None` if no sample has arrived since the last call. Default
    /// no-op for non-BLE transports.
    fn poll_phone_gps_sample(&mut self) -> Result<Option<Vec<u8>>, RouteSyncIoError> {
        Ok(None)
    }

    /// Tell the BLE controller whether to lock its advertising-filter
    /// policy to a whitelist (after a successful bond) or open it back
    /// up to any scanner (after the bond is dropped). Default no-op
    /// for non-BLE transports. The hosted-BLE impl forwards to
    /// `hosted_ble_route_sync_set_adv_filter`.
    fn set_advertising_allowlist(
        &mut self,
        _peer_addr: Option<([u8; 6], u8)>,
    ) -> Result<(), RouteSyncIoError> {
        Ok(())
    }
}

/// Platform-agnostic shape of a Bluedroid `auth_cmpl` event. Mirrors
/// `crate::hosted_ble::AuthCmplEvent` but kept here so the trait is
/// usable from non-esp32p4 builds (host tests, parity fixtures).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AuthCmplOutcome {
    pub success: bool,
    pub fail_reason: u8,
    pub peer_addr: [u8; 6],
    pub addr_type: u8,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct NullRouteSyncIo;

impl RouteSyncIo for NullRouteSyncIo {
    fn poll_chunk(&mut self) -> Result<Option<RouteTransferChunk>, RouteSyncIoError> {
        Ok(None)
    }

    fn publish_messages(&mut self, _messages: &[RouteSyncMessage]) -> Result<(), RouteSyncIoError> {
        Ok(())
    }
}

/// Battery fuel-gauge source, polled by the platform at a throttled
/// cadence and pushed into the App for the corner readout. `read`
/// returns `None` when there is nothing new worth showing; the
/// device's MAX17048 driver always returns `Some` (its reading carries
/// a `present` flag for wiring failures) while the host/emulator null
/// source always returns `None`.
pub trait FuelGaugeSource {
    fn read(&mut self) -> Option<FuelGaugeReading>;
}

/// No-op fuel gauge for host builds, tests, and the emulator.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct NullFuelGauge;

impl FuelGaugeSource for NullFuelGauge {
    fn read(&mut self) -> Option<FuelGaugeReading> {
        None
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
    F = NullFuelGauge,
> where
    T: TouchSource,
    G: GpsProvider,
    C: FrameClock,
    R: RouteSyncIo,
    S: MapSource,
    B: DisplayBackend,
    U: SettingsStore,
    F: FuelGaugeSource,
{
    app: App<S, B, U>,
    touch: T,
    gps: G,
    clock: C,
    route_sync: R,
    fuel_gauge: F,
    /// Accumulates frame `dt` between fuel-gauge polls so the I²C
    /// read runs at the configured cadence, not 60 Hz.
    fuel_gauge_poll_accumulator: Duration,
    /// Poll cadence, set at construction from
    /// `FuelGaugeConfig::poll_interval` (device) or a 2 s default
    /// (host/emulator, where the null source is free anyway).
    fuel_gauge_poll_interval: Duration,
    last_reroute_requested: bool,
    /// Monotonic instant of the last valid phone GPS sample received
    /// from the companion. Used to detect BLE disconnect / sample
    /// stream interruption and auto-fallback to internal GPS.
    phone_gps_last_received: Option<Instant>,
    /// Whether the most recent touch poll failed. Gates the per-frame
    /// error log so a glitching touch controller produces one warn on
    /// the healthy→failing edge and one info on recovery, instead of a
    /// line per frame while the screen stays responsive.
    touch_failing: bool,
}

impl<T, G, C, S, B, U, R, F> RuntimePlatform<T, G, C, S, B, U, R, F>
where
    T: TouchSource,
    G: GpsProvider,
    C: FrameClock,
    S: MapSource,
    B: DisplayBackend,
    U: SettingsStore,
    R: RouteSyncIo,
    F: FuelGaugeSource,
{
    /// Full constructor with an explicit fuel gauge. The device
    /// entrypoint uses this to install the MAX17048 driver; the
    /// gauge-less convenience constructors below fix `F` to
    /// [`NullFuelGauge`].
    pub fn with_route_sync_and_fuel_gauge(
        app: App<S, B, U>,
        touch: T,
        gps: G,
        clock: C,
        route_sync: R,
        fuel_gauge: F,
        fuel_gauge_poll_interval: Duration,
    ) -> Self {
        Self {
            app,
            touch,
            gps,
            clock,
            route_sync,
            fuel_gauge,
            fuel_gauge_poll_accumulator: Duration::ZERO,
            fuel_gauge_poll_interval,
            last_reroute_requested: false,
            phone_gps_last_received: None,
            touch_failing: false,
        }
    }

    pub fn run_frame(&mut self) -> Result<FrameResult, PlatformError> {
        // Companion wrote `pairing_request` since the last frame —
        // open the QR-display window before we render so the user
        // sees the QR on this same frame.
        if self.route_sync.poll_pairing_request()? {
            log::info!("platform.run_frame — pairing_request flag drained; calling request_qr_display");
            self.app.request_qr_display();
        }
        // Drain any pairing-confirm secrets the BT host task delivered.
        // On a match the App transitions to Operational and clears the
        // QR display window.
        while let Some(secret) = self.route_sync.poll_pairing_secret()? {
            log::info!(
                "platform.run_frame — pairing-confirm secret drained ({} B); calling ingest_pairing_confirm",
                secret.len()
            );
            let _ = self.app.ingest_pairing_confirm(&secret);
        }

        // Drain SMP auth-completion events so a successful bond locks
        // in the peer identity and a failure drops the prior bond +
        // returns to unbonded. After the App has reacted to the
        // outcome, push the resulting allowlist policy to the
        // controller: bonded ⇒ whitelist-only with the bonded BD_ADDR,
        // unbonded ⇒ open advertising.
        let mut allowlist_dirty = false;
        while let Some(outcome) = self.route_sync.poll_auth_cmpl()? {
            log::info!(
                "platform.run_frame — auth_cmpl drained: success={} reason={:#x}",
                outcome.success,
                outcome.fail_reason,
            );
            self.app.ingest_auth_cmpl(outcome);
            allowlist_dirty = true;
        }
        if allowlist_dirty {
            let peer = self.app.bonded_peer_for_allowlist();
            if let Err(error) = self.route_sync.set_advertising_allowlist(peer) {
                log::warn!(
                    "set_advertising_allowlist({:?}) failed: {:?}",
                    peer,
                    error,
                );
            }
        }

        let mut route_sync_statuses = Vec::new();
        while let Some(chunk) = self.route_sync.poll_chunk()? {
            match self.app.ingest_route_sync_chunk(chunk) {
                Ok(statuses) => route_sync_statuses.extend(statuses),
                Err(error) => route_sync_statuses.push(error.as_status_message()),
            }
        }

        let dt = self.clock.next_dt();

        // --- Phone GPS ---
        // Drain the most recent phone GPS sample from the BLE queue. If
        // the companion is streaming samples, use the latest one instead
        // of the internal GPS provider. Auto-fallback to Internal if no
        // sample arrives within the timeout window (handles BLE disconnect
        // and companion backgrounding gracefully).
        let mut phone_gps_input: Option<GpsInput> = None;
        while let Some(raw) = self.route_sync.poll_phone_gps_sample()? {
            match parse_phone_gps_csv(&raw) {
                Ok(sample) => {
                    phone_gps_input = Some(sample);
                    self.phone_gps_last_received = Some(Instant::now());
                    // Switch to phone GPS on the first valid sample.
                    if self.app.gps_source() != GpsSource::Phone {
                        self.app.set_gps_source(GpsSource::Phone);
                    }
                }
                Err(_) => {
                    // Malformed CSV — drop silently, next write will
                    // deliver a fresh sample.
                }
            }
        }

        // Auto-fallback: no phone sample for > 120 s → revert to Internal.
        // Route-transfer bursts and mobile scheduling can temporarily delay
        // phone GPS writes; keep the timeout long enough to avoid false
        // fallback during an active companion session.
        const PHONE_GPS_TIMEOUT: Duration = Duration::from_secs(120);
        if self.app.gps_source() == GpsSource::Phone {
            if let Some(last) = self.phone_gps_last_received {
                if last.elapsed() > PHONE_GPS_TIMEOUT {
                    log::info!(
                        "Phone GPS timed out (no sample for {:?}s) — reverting to Internal",
                        last.elapsed().as_secs()
                    );
                    self.app.set_gps_source(GpsSource::Internal);
                }
            }
        }

        let gps = if self.app.gps_source() == GpsSource::Phone {
            phone_gps_input
        } else {
            self.gps.poll()?
        };

        // Push diagnostics into the App every frame so the overlay
        // can render the BITS/PING/PINS counters (internal GPS only).
        // Phone GPS has no diagnostics and is always considered acquired.
        if self.app.gps_source() == GpsSource::Phone {
            self.app.set_gps_diagnostics(None);
            self.app.set_gps_acquired(true);
        } else {
            let diagnostics = self.gps.diagnostics_summary();
            self.app.set_gps_diagnostics(diagnostics);
            let acquired = match diagnostics {
                Some(diag) => diag
                    .last_fix_age_ms
                    .is_some_and(|age| age <= GPS_LOST_THRESHOLD_MS),
                None => self.gps.has_acquired_fix(),
            };
            self.app.set_gps_acquired(acquired);
        }
        // Touch reads are best-effort: a single I²C glitch must not
        // kill the whole device (observed on the 3.4C where one GT911
        // read error took the nav screen down). On failure we log
        // once, skip the touch sample for this frame, and keep
        // rendering — the next frame retries.
        let touch = match self.touch.poll() {
            Ok(touch) => {
                if self.touch_failing {
                    self.touch_failing = false;
                    log::info!("platform: touch controller recovered — input back online");
                }
                touch
            }
            Err(error) => {
                if !self.touch_failing {
                    self.touch_failing = true;
                    log::warn!(
                        "platform: touch read failed ({error:?}) — ignoring touch input \
                         until the controller recovers"
                    );
                }
                None
            }
        };

        // Throttled fuel-gauge poll. `NullFuelGauge` returns None
        // immediately so host/emulator frames pay nothing; the device
        // driver performs the I²C read at `poll_interval` cadence and
        // the result flows into the persistent corner readout.
        self.fuel_gauge_poll_accumulator += dt;
        if self.fuel_gauge_poll_accumulator >= self.fuel_gauge_poll_interval {
            self.fuel_gauge_poll_accumulator = Duration::ZERO;
            if let Some(reading) = self.fuel_gauge.read() {
                self.app.set_fuel_gauge_reading(reading);
            }
        }

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
            outbound_messages.push(RouteSyncMessage::RerouteRequest(
                RouteRerouteRequestMessage {
                    route_id: frame.output.route.route_id.clone(),
                    rider_position: GeoPoint::new(gps.lat_deg, gps.lon_deg),
                    reason: "off_route".to_owned(),
                },
            ));
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

    pub fn app_mut(&mut self) -> &mut App<S, B, U> {
        &mut self.app
    }

    pub fn route_sync(&self) -> &R {
        &self.route_sync
    }
}

impl<T, G, C, S, B, U, R> RuntimePlatform<T, G, C, S, B, U, R, NullFuelGauge>
where
    T: TouchSource,
    G: GpsProvider,
    C: FrameClock,
    S: MapSource,
    B: DisplayBackend,
    U: SettingsStore,
    R: RouteSyncIo,
{
    /// Gauge-less constructor for host builds, the emulator, and
    /// tests. Fixes the fuel-gauge slot to [`NullFuelGauge`] (whose
    /// `read()` returns `None` instantly) and the poll cadence to a
    /// 2 s default.
    pub fn with_route_sync(app: App<S, B, U>, touch: T, gps: G, clock: C, route_sync: R) -> Self {
        Self::with_route_sync_and_fuel_gauge(
            app,
            touch,
            gps,
            clock,
            route_sync,
            NullFuelGauge,
            Duration::from_secs(2),
        )
    }
}

impl<T, G, C, S, B, U> RuntimePlatform<T, G, C, S, B, U, NullRouteSyncIo, NullFuelGauge>
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

/// Parse a phone GPS CSV string into a [`GpsInput`].
///
/// Format: `lat,lon,speed,course,accuracy` — e.g.
/// `60.174420,24.942100,5.0,0.0,4.0`. Course and accuracy may be
/// missing or empty (coerced to `None`).
fn parse_phone_gps_csv(raw: &[u8]) -> Result<GpsInput, GpsError> {
    let text = std::str::from_utf8(raw)
        .map_err(|_| GpsError::Provider("phone GPS: invalid UTF-8".into()))?;
    let fields: Vec<&str> = text.trim().split(',').collect();
    if fields.len() < 3 {
        return Err(GpsError::Provider(
            "phone GPS: need at least lat,lon,speed".into(),
        ));
    }
    let lat_deg = fields[0]
        .parse::<f64>()
        .map_err(|_| GpsError::Provider("phone GPS: invalid lat".into()))?;
    let lon_deg = fields[1]
        .parse::<f64>()
        .map_err(|_| GpsError::Provider("phone GPS: invalid lon".into()))?;
    let speed_mps = fields[2]
        .parse::<f32>()
        .map_err(|_| GpsError::Provider("phone GPS: invalid speed".into()))?;
    let course_rad = fields
        .get(3)
        .filter(|s| !s.is_empty())
        .and_then(|s| s.parse::<f32>().ok())
        .map(|deg| deg.to_radians());
    let horizontal_accuracy_m = fields
        .get(4)
        .filter(|s| !s.is_empty())
        .and_then(|s| s.parse::<f32>().ok());
    Ok(GpsInput {
        lat_deg,
        lon_deg,
        speed_mps,
        course_rad,
        horizontal_accuracy_m,
    })
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

    /// Touch source whose first polls can be made to fail (simulating
    /// a GT911 I²C glitch) and then recover.
    #[derive(Debug)]
    struct FlakyTouchSource {
        failures: VecDeque<bool>,
    }

    impl FlakyTouchSource {
        fn new(failures: impl IntoIterator<Item = bool>) -> Self {
            Self {
                failures: failures.into_iter().collect(),
            }
        }
    }

    impl TouchSource for FlakyTouchSource {
        fn poll(&mut self) -> Result<Option<TouchInput>, TouchError> {
            if self.failures.pop_front() == Some(true) {
                return Err(TouchError::Controller("simulated i2c glitch".into()));
            }
            Ok(Some(TouchInput {
                sequence: 1,
                contacts: vec![crate::touch::RawTouchContact {
                    id: 1,
                    phase: TouchPhase::Started,
                    position: ScreenPoint::new(100.0, 100.0),
                    pressure: Some(0.5),
                }],
            }))
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
    fn runtime_platform_survives_touch_read_glitches() {
        let board = crate::board_config::BoardConfig::default();
        let app = App::default();
        // First poll glitches (the 3.4C GT911 failure mode), then the
        // controller recovers on the next frames.
        let touch = FlakyTouchSource::new([true, false, false]);
        let mut platform = RuntimePlatform::new(
            app,
            touch,
            NullGpsProvider,
            FixedFrameClock::new(board.frame_interval),
        );

        // The glitch frame must not fail the platform — the device
        // keeps rendering with the touch sample dropped for one frame.
        let frame = platform
            .run_frame()
            .expect("touch glitch must not kill the platform");
        assert_eq!(frame.output.frame_index, 1);
        assert!(platform.app().display().presented_frames() >= 1);

        // Recovery frames keep advancing and deliver touch input again.
        let frame = platform
            .run_frame()
            .expect("post-glitch frame must run");
        assert_eq!(frame.output.frame_index, 2);
        assert!(platform.app().display().presented_frames() >= 2);
    }

    #[test]
    fn runtime_platform_forwards_route_sync_between_transport_and_app() {
        let board = crate::board_config::BoardConfig::default();
        let app = App::default();
        let message = RouteSyncMessage::Set(RouteSetMessage {
            route: sample_route(1),
        });
        let route_sync = SequenceRouteSyncIo::new(chunk_sync_message(&message, "transfer-1", 32));
        let mut platform = RuntimePlatform::with_route_sync_and_fuel_gauge(
            app,
            NullTouchSource,
            NullGpsProvider,
            FixedFrameClock::new(board.frame_interval),
            route_sync,
            NullFuelGauge,
            Duration::from_secs(2),
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
        let mut platform = RuntimePlatform::with_route_sync_and_fuel_gauge(
            app,
            NullTouchSource,
            NullGpsProvider,
            FixedFrameClock::new(board.frame_interval),
            route_sync,
            NullFuelGauge,
            Duration::from_secs(2),
        );

        let frame = platform
            .run_frame()
            .expect("platform frame should survive checksum mismatch");

        assert_eq!(frame.route_sync_statuses.len(), 1);
        assert_eq!(
            frame.route_sync_statuses[0].status,
            RouteSyncStatusCode::RetryableFailure
        );
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
route_id=broken-only"
            .to_vec();
        let chunk = RouteTransferChunk {
            transfer_id: "broken-transfer".to_owned(),
            chunk_index: 0,
            total_chunks: 1,
            checksum_hex: crate::route_sync::checksum_hex(&payload),
            payload_fragment: payload,
        };
        let route_sync = SequenceRouteSyncIo::new([chunk]);
        let mut platform = RuntimePlatform::with_route_sync_and_fuel_gauge(
            app,
            NullTouchSource,
            NullGpsProvider,
            FixedFrameClock::new(board.frame_interval),
            route_sync,
            NullFuelGauge,
            Duration::from_secs(2),
        );

        let frame = platform
            .run_frame()
            .expect("platform frame should survive malformed payload");

        assert_eq!(frame.route_sync_statuses.len(), 1);
        assert_eq!(
            frame.route_sync_statuses[0].status,
            RouteSyncStatusCode::FatalFailure
        );
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
        let mut platform = RuntimePlatform::with_route_sync_and_fuel_gauge(
            app,
            NullTouchSource,
            gps,
            FixedFrameClock::new(Duration::from_secs(3)),
            route_sync,
            NullFuelGauge,
            Duration::from_secs(2),
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
