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
use crate::gps::{GpsDiagnostics, GpsInput, GpsSource};
use crate::input_bridge::InputBridge;
use crate::map_source::MapSourceBridge;
use crate::pairing::{
    PERIPHERAL_ADDRESS_LEN, PairingStateMachine, SECRET_LEN, Transition,
};
use crate::platform::AuthCmplOutcome;
use crate::route_sync::{RouteSyncTransport, RouteSyncTransportError, RouteTransferChunk};
use crate::settings::{DeviceSettings, NullSettingsStore, SettingsError, SettingsStore};
use crate::touch::TouchInput;
use crate::world_buffer::{WorldBuffer, render_ui_overlay};

/// Deterministic placeholder used by host builds + tests. The device
/// entrypoint replaces this with an ESP-IDF `esp_fill_random`-backed
/// factory so production secrets come from the hardware RNG.
fn zero_secret() -> [u8; SECRET_LEN] {
    [0u8; SECRET_LEN]
}

/// Identity computed for a successful pairing-confirm match. Production
/// would derive this from the connecting peer's BD_ADDR + IRK; for now
/// any non-zero value is fine — the iOS / Android companion stores the
/// same identity in its own paired-peripheral record.
fn placeholder_peer_identity() -> [u8; 16] {
    [0u8; 16]
}

/// How long the device shows its QR after a companion writes
/// `pairing_request`. Long enough for the user to read instructions
/// and align the camera; short enough that the QR doesn't linger if
/// the user abandoned the flow.
pub const QR_DISPLAY_DURATION: Duration = Duration::from_secs(90);

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
    /// QR-OOB pairing state machine. Owns the current ephemeral secret
    /// (when in pairing mode) or the bonded peer's identity (when
    /// `Operational`). Driven from `step_frame`: rotates the secret
    /// once per `ROTATION_PERIOD`, drains the inbound pairing-confirm
    /// queue.
    pairing: PairingStateMachine,
    /// Pluggable factory for fresh ephemeral pairing secrets. Production
    /// (esp-idf) wires this to `esp_fill_random`; host builds and tests
    /// use `zero_secret` so the QR is deterministic.
    pairing_secret_factory: fn() -> [u8; SECRET_LEN],
    /// Diagnostics: number of route-sync chunks rejected because the
    /// device was still in pairing mode. Exposed to tests / settings UI.
    pairing_dropped_chunks: u32,
    /// While `Some(deadline)` and `monotonic_clock < deadline`, the
    /// device renders the QR overlay instead of the map. Set when the
    /// companion writes `pairing_request` (UUID …-1005); cleared on a
    /// successful pairing-confirm match or when the deadline elapses.
    /// `None` is the default — the device shows the map until a
    /// companion explicitly asks to enter pairing mode.
    qr_display_deadline: Option<Duration>,
    /// BD_ADDR of the most recent peer that completed SMP successfully.
    /// Captured from the `ESP_GAP_BLE_AUTH_CMPL_EVT(success)` handler
    /// and used downstream to push the bonded peer into the
    /// controller's whitelist for allowlist-only advertising.
    last_bonded_peer: Option<[u8; 6]>,
    /// Address-type byte that pairs with `last_bonded_peer` (public vs.
    /// random-resolvable). Tracked separately because the whitelist
    /// API takes `(bd_addr, addr_type)`.
    last_bonded_peer_addr_type: Option<u8>,
    /// Tracks whether the previous frame entered the pairing-overlay path
    /// so we can emit a single info-level log on the transition (instead
    /// of spamming the 60 Hz render loop).
    was_in_pairing_overlay: bool,
    /// One-shot flag for the first successful QR encode so the operator
    /// gets a single confirmation in the boot log instead of 60/s spam.
    qr_encoded_logged: bool,
    /// Whether the GPS provider has ever produced a real fix. Set by
    /// the platform layer each frame from
    /// [`crate::gps::GpsProvider::has_acquired_fix`]. While `false`,
    /// `step_frame` paints the "GETTING GPS" overlay over the basemap
    /// after the normal render pass — the rider sees a Helsinki-area
    /// map with a black/white "GETTING GPS" banner until the NEO-6M
    /// reports its first valid RMC sentence.
    gps_acquired: bool,
    /// Tracks the previous-frame value so we can emit a single info
    /// log on the false→true edge instead of a per-frame line.
    gps_acquired_logged: bool,
    /// Latest counters from the GPS provider, surfaced on the
    /// "GETTING GPS" overlay so a field operator can debug "no fix
    /// after 30 minutes outdoors" without serial-console access.
    /// `None` while the provider doesn't expose diagnostics
    /// (host tests, `NullGpsProvider`, etc.).
    gps_diagnostics: Option<GpsDiagnostics>,
    /// Current GPS source for this session. Resets to `Internal` on
    /// every boot. When `Phone`, the platform layer uses phone GPS
    /// samples instead of polling the internal GPS provider, and the
    /// "GETTING GPS" overlay is suppressed.
    gps_source: GpsSource,
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
    /// Pairing-mode QR overlay; the basemap and runtime are skipped
    /// entirely until the device is bonded.
    PairingOverlay,
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
        log::info!(
            "App::new — loaded settings: device_paired={}, has_peer_identity={}",
            persisted_settings.device_paired,
            persisted_settings.peer_identity.is_some(),
        );
        // The board's BLE peripheral address comes from the GAP layer
        // at runtime (`esp_ble_gap_get_local_used_addr`); host builds
        // and tests use a stable placeholder so the QR payload is
        // reproducible. The device entrypoint replaces this via
        // `App::set_peripheral_address` once Bluedroid is up.
        let peripheral_address = [0x00u8; PERIPHERAL_ADDRESS_LEN];
        let initial_secret = zero_secret();
        let pairing = if persisted_settings.device_paired {
            log::info!("App::new — device_paired=true → starting in Operational mode (no QR)");
            PairingStateMachine::new_paired(
                peripheral_address,
                persisted_settings.peer_identity.unwrap_or([0u8; 16]),
            )
        } else {
            log::info!(
                "App::new — device_paired=false → unbonded; map renders by default. \
                 QR will only show after a companion writes pairing_request (UUID …-1005)"
            );
            PairingStateMachine::new_unpaired(
                peripheral_address,
                initial_secret,
                Duration::ZERO,
            )
        };
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
            pairing,
            pairing_secret_factory: zero_secret,
            pairing_dropped_chunks: 0,
            qr_display_deadline: None,
            last_bonded_peer: None,
            last_bonded_peer_addr_type: None,
            was_in_pairing_overlay: false,
            qr_encoded_logged: false,
            // Default `true` so unit tests that drive `step_frame`
            // directly (bypassing the platform layer) don't see the
            // overlay covering pixels they care about. In production
            // `RuntimePlatform::run_frame` calls
            // `App::set_gps_acquired(self.gps.has_acquired_fix())`
            // on every frame *before* `step_frame`, so the constructor
            // default is overwritten on frame 1 and the rider sees
            // "GETTING GPS" until the NEO-6M reports its first fix.
            gps_acquired: true,
            gps_acquired_logged: false,
            gps_diagnostics: None,
            gps_source: GpsSource::Internal,
            #[cfg(test)]
            last_render_path: FramePath::None,
        })
    }

    /// Replace the placeholder peripheral address baked in at
    /// construction with the real BD_ADDR returned by Bluedroid. Called
    /// once during boot after Bluedroid is up; rebuilds the pairing
    /// state machine in-place so the QR payload encodes the right
    /// address. No-op if the device is already bonded — the address is
    /// only used for the QR.
    pub fn set_peripheral_address(&mut self, address: [u8; PERIPHERAL_ADDRESS_LEN]) {
        if self.pairing.is_paired() {
            log::info!(
                "set_peripheral_address: already paired, keeping bond (BD_ADDR {:02X}:{:02X}:{:02X}:{:02X}:{:02X}:{:02X})",
                address[0], address[1], address[2], address[3], address[4], address[5]
            );
            return;
        }
        log::info!(
            "set_peripheral_address: rebuilt pairing state with BD_ADDR {:02X}:{:02X}:{:02X}:{:02X}:{:02X}:{:02X} — QR payload now valid",
            address[0], address[1], address[2], address[3], address[4], address[5]
        );
        self.pairing = PairingStateMachine::new_unpaired(
            address,
            (self.pairing_secret_factory)(),
            self.monotonic_clock,
        );
    }

    /// Override the pairing-secret RNG. The device entrypoint sets this
    /// to a thin wrapper around `esp_fill_random` so production secrets
    /// come from the hardware RNG.
    pub fn set_pairing_secret_factory(&mut self, factory: fn() -> [u8; SECRET_LEN]) {
        self.pairing_secret_factory = factory;
    }

    /// Ingest a pairing-confirm secret pulled off the inbound BLE queue
    /// (or, in tests, a synthetic one). Returns the resulting state
    /// transition; the caller is responsible for persisting the bond
    /// when the transition is `Bonded`.
    pub fn ingest_pairing_confirm(&mut self, payload: &[u8]) -> Transition {
        let transition = self
            .pairing
            .on_pairing_confirm(payload, placeholder_peer_identity);
        if let Transition::Bonded { peer_identity } = &transition {
            let _ = self.mark_paired(*peer_identity);
            // Successful bond → return the panel to the map even if
            // the QR-display window hadn't elapsed yet.
            self.qr_display_deadline = None;
        }
        transition
    }

    /// Forward an SMP `auth_cmpl` outcome from Bluedroid into the
    /// pairing state machine. Two paths converge with
    /// `ingest_pairing_confirm`:
    ///
    /// * On `success == true` the link is encrypted and Bluedroid
    ///   has stored the peer's keys in its own NVS namespace. The OOB
    ///   secret check (already passed via `pairing_confirm`) is what
    ///   actually authorizes the bond at the application layer; this
    ///   handler just records the bonded peer's BD_ADDR so we can
    ///   later push it into the controller's whitelist.
    /// * On `success == false` while we currently think we're bonded
    ///   (the bonded phone forgot us, or its IRK rotated) we drop the
    ///   bond from NVS and return to unbonded so the next companion
    ///   write of `pairing_request` opens the QR cleanly.
    pub fn ingest_auth_cmpl(&mut self, outcome: AuthCmplOutcome) -> Transition {
        if outcome.success {
            log::info!(
                "App.ingest_auth_cmpl — SMP success bd={:02x}:{:02x}:{:02x}:{:02x}:{:02x}:{:02x}",
                outcome.peer_addr[0],
                outcome.peer_addr[1],
                outcome.peer_addr[2],
                outcome.peer_addr[3],
                outcome.peer_addr[4],
                outcome.peer_addr[5],
            );
            self.last_bonded_peer = Some(outcome.peer_addr);
            self.last_bonded_peer_addr_type = Some(outcome.addr_type);
            return Transition::None;
        }
        log::warn!(
            "App.ingest_auth_cmpl — SMP failure reason=0x{:02x}; \
             dropping bond + returning to unbonded if currently paired",
            outcome.fail_reason,
        );
        let transition = self.pairing.on_auth_failure();
        if matches!(transition, Transition::ClearBondAndPair) {
            let next = DeviceSettings {
                device_paired: false,
                peer_identity: None,
                ..self.persisted_settings
            };
            if next != self.persisted_settings {
                if let Err(error) = self.settings_store.save_settings(&next) {
                    log::warn!(
                        "settings_store.save_settings on auth-failure unbond: {error:?}"
                    );
                }
                self.persisted_settings = next;
            }
            self.last_bonded_peer = None;
            self.last_bonded_peer_addr_type = None;
        }
        transition
    }

    /// Persist a fresh bond to the settings store and flip the
    /// in-memory `device_paired` flag so subsequent boots come up
    /// `Operational` rather than back in pairing mode.
    fn mark_paired(&mut self, peer_identity: [u8; 16]) -> Result<(), SettingsError> {
        let next = DeviceSettings {
            device_paired: true,
            peer_identity: Some(peer_identity),
            ..self.persisted_settings
        };
        if next != self.persisted_settings {
            self.settings_store.save_settings(&next)?;
            self.persisted_settings = next;
        }
        Ok(())
    }

    /// True if the device has not yet bonded with a companion. Route
    /// writes are dropped in this state; the runtime, map query, and
    /// render pipeline still run normally.
    pub fn is_unbonded(&self) -> bool {
        !self.pairing.is_paired()
    }

    /// Backwards-compatible alias for [`Self::is_unbonded`]. The QR
    /// overlay is no longer driven by bond state — it's driven by
    /// [`Self::is_showing_qr`] — but a few existing tests still read
    /// this method to mean "device is in the unbonded half of the
    /// pairing state machine".
    pub fn is_in_pairing_mode(&self) -> bool {
        self.is_unbonded()
    }

    /// True while the QR overlay is currently being rendered. The
    /// device enters this state when a companion writes the
    /// `pairing_request` characteristic; it leaves on a successful
    /// pairing-confirm match or after [`QR_DISPLAY_DURATION`] elapses
    /// since the request.
    pub fn is_showing_qr(&self) -> bool {
        self.qr_display_deadline
            .map(|deadline| self.monotonic_clock < deadline)
            .unwrap_or(false)
    }

    /// Companion-driven trigger: a `pairing_request` write hit the GATT
    /// server. Show the QR for the next [`QR_DISPLAY_DURATION`] so the
    /// user can scan it. If the device is still unbonded the existing
    /// secret is reused; if it was bonded, the bond is dropped + a
    /// fresh secret is generated so the resulting QR carries new bytes
    /// (otherwise a stale photograph from a previous pairing session
    /// could be replayed).
    pub fn request_qr_display(&mut self) {
        let now = self.monotonic_clock;
        self.qr_display_deadline = Some(now + QR_DISPLAY_DURATION);
        if self.pairing.is_paired() {
            // Re-pair flow: companion explicitly asked to bond again.
            // Drop the prior bond locally so a fresh handshake produces
            // a fresh peer_identity.
            let factory = self.pairing_secret_factory;
            self.pairing = PairingStateMachine::new_unpaired(
                [0u8; PERIPHERAL_ADDRESS_LEN],
                factory(),
                now,
            );
            let next = DeviceSettings {
                device_paired: false,
                peer_identity: None,
                ..self.persisted_settings
            };
            if next != self.persisted_settings {
                if let Err(error) = self.settings_store.save_settings(&next) {
                    log::warn!("settings_store.save_settings on re-pair: {error:?}");
                }
                self.persisted_settings = next;
            }
            log::info!(
                "request_qr_display — was bonded; dropped bond and generated fresh secret"
            );
        } else {
            log::info!("request_qr_display — showing QR for {:?}", QR_DISPLAY_DURATION);
        }
    }

    /// Records whether the GPS provider has produced a real fix yet. The
    /// platform layer calls this once per frame from
    /// [`crate::gps::GpsProvider::has_acquired_fix`]; while the value is
    /// `false`, [`Self::step_frame`] paints the "GETTING GPS" overlay
    /// over the map after the normal render pass.
    pub fn set_gps_acquired(&mut self, acquired: bool) {
        let was_acquired = self.gps_acquired;
        self.gps_acquired = acquired;
        // Log only on the false → true edge: that's the moment the
        // operator cares about ("GPS just locked"). A constructor that
        // boots with `gps_acquired = true` never logs because there
        // was no acquisition event to announce.
        if acquired && !was_acquired && !self.gps_acquired_logged {
            log::info!("App: first real GPS fix received → dropping GETTING GPS overlay");
            self.gps_acquired_logged = true;
        }
    }

    pub fn is_gps_acquired(&self) -> bool {
        self.gps_acquired
    }

    /// Stash the latest counters from the GPS provider. Called once
    /// per frame from the platform layer; the value flows into the
    /// "GETTING GPS" overlay so an operator standing outdoors can see
    /// whether bytes are arriving (diagnoses wiring), sentences parse
    /// (diagnoses baud), and fixes resolve (diagnoses sky view).
    pub fn set_gps_diagnostics(&mut self, diagnostics: Option<GpsDiagnostics>) {
        self.gps_diagnostics = diagnostics;
    }

    pub fn gps_diagnostics(&self) -> Option<GpsDiagnostics> {
        self.gps_diagnostics
    }

    /// Set the GPS source for this session. Switching to `Phone`
    /// suppresses the "GETTING GPS" overlay and causes the platform
    /// layer to use phone GPS samples instead of the internal provider.
    pub fn set_gps_source(&mut self, source: GpsSource) {
        if source != self.gps_source {
            log::info!("App: GPS source switched to {:?}", source);
            self.gps_source = source;
        }
    }

    pub fn gps_source(&self) -> GpsSource {
        self.gps_source
    }

    /// Test-only: shortcut into the `Operational` state without going
    /// through the QR-OOB handshake. Used by smoke tests that just want
    /// to exercise the runtime/render path; pairing-flow coverage lives
    /// in dedicated tests below.
    #[cfg(test)]
    pub fn force_paired_for_test(&mut self) {
        self.pairing = PairingStateMachine::new_paired(
            [0u8; PERIPHERAL_ADDRESS_LEN],
            [0u8; 16],
        );
        self.persisted_settings.device_paired = true;
        self.persisted_settings.peer_identity = Some([0u8; 16]);
    }

    #[cfg(test)]
    pub fn is_paired_for_test(&self) -> bool {
        self.pairing.is_paired()
    }

    #[cfg(test)]
    pub fn last_bonded_peer_for_test(&self) -> Option<([u8; 6], u8)> {
        self.bonded_peer_for_allowlist()
    }

    /// BD_ADDR + addr-type of the currently-bonded peer if we have
    /// both pieces (set after a successful `auth_cmpl`). Returns
    /// `None` while unbonded so the platform layer programs the
    /// controller's advertising filter back to "accept any scanner".
    pub fn bonded_peer_for_allowlist(&self) -> Option<([u8; 6], u8)> {
        if !self.pairing.is_paired() {
            return None;
        }
        match (self.last_bonded_peer, self.last_bonded_peer_addr_type) {
            (Some(bd), Some(addr_type)) => Some((bd, addr_type)),
            _ => None,
        }
    }

    /// Diagnostics counter — number of route-sync chunks dropped because
    /// they arrived while the device was still in pairing mode.
    pub fn pairing_dropped_chunks(&self) -> u32 {
        self.pairing_dropped_chunks
    }

    /// The bytes the QR overlay needs to encode. `None` once bonded.
    pub fn current_qr_payload(&self) -> Option<Vec<u8>> {
        self.pairing.current_qr_payload()
    }

    pub fn step_frame(
        &mut self,
        dt: Duration,
        gps: Option<GpsInput>,
        touch: Option<TouchInput>,
    ) -> Result<FrameResult, AppError> {
        // Advance the monotonic clock first so all per-frame timers
        // (route-sync idle, pairing rotation) share one source of
        // truth.
        self.monotonic_clock = self.monotonic_clock.saturating_add(dt);

        // The pairing state machine still ticks every frame so the
        // ephemeral secret rotates while the device is unbonded — the
        // QR display window may be opened on short notice and we want
        // the secret to be fresh.
        let pairing_factory = self.pairing_secret_factory;
        self.pairing.tick(self.monotonic_clock, || pairing_factory());

        // Auto-expire the QR-display deadline so the device returns to
        // the map after the timeout if no bond completed.
        if let Some(deadline) = self.qr_display_deadline {
            if self.monotonic_clock >= deadline {
                self.qr_display_deadline = None;
                log::info!("QR display deadline elapsed — returning to map");
            }
        }

        // QR-overlay path: only when a companion explicitly asked the
        // device to enter pairing mode (via `pairing_request`). The
        // runtime and route-sync apply path are bypassed for this
        // window; the user is meant to scan the QR with their phone.
        if self.is_showing_qr() {
            if !self.was_in_pairing_overlay {
                log::info!(
                    "step_frame → entering QR overlay (clock={}ms)",
                    self.monotonic_clock.as_millis()
                );
                self.was_in_pairing_overlay = true;
            }
            return self.render_pairing_overlay();
        }
        if self.was_in_pairing_overlay {
            log::info!("step_frame → leaving QR overlay");
            self.was_in_pairing_overlay = false;
        }

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
        // While the NEO-6M is still searching for satellites, paint
        // the "GETTING GPS" banner over whatever the renderer just
        // produced. The map (camera held on the seed lat/lon by
        // `SeedThenRealGpsProvider`) stays visible underneath; the
        // overlay only overwrites the centered panel footprint, and
        // disappears as soon as the provider reports its first real
        // fix and `set_gps_acquired(true)` is called.
        // While GPS is still searching (internal mode only), paint the
        // "GETTING GPS" banner. Phone GPS is always considered acquired
        // so the overlay never appears in that mode.
        if self.gps_source != GpsSource::Phone && !self.gps_acquired {
            crate::gps_overlay::render_acquiring_gps_overlay(
                &mut self.render_framebuffer,
                self.gps_diagnostics,
            );
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

    /// Pairing-mode short-circuit. Renders the QR overlay onto the
    /// panel and returns a placeholder `FrameResult` so the platform
    /// driver keeps pushing frames at the normal cadence even while
    /// the runtime is paused.
    fn render_pairing_overlay(&mut self) -> Result<FrameResult, AppError> {
        let t_render_start = Instant::now();
        self.render_framebuffer
            .clear(render_core::style::COLOR_BACKGROUND_CANVAS);
        if let Some(payload) = self.pairing.current_qr_payload() {
            // The state machine yields the canonical binary
            // `peripheral_address || secret` form; the QR ships JSON so
            // the cross-platform companion decoders share one schema
            // (see `parity-fixtures/data/pairing_qr_v1.json`).
            let mut address = [0u8; PERIPHERAL_ADDRESS_LEN];
            address.copy_from_slice(&payload[..PERIPHERAL_ADDRESS_LEN]);
            let mut secret = [0u8; SECRET_LEN];
            secret.copy_from_slice(&payload[PERIPHERAL_ADDRESS_LEN..]);
            let json = crate::pairing_overlay::format_qr_json(
                address,
                secret,
                env!("CARGO_PKG_VERSION"),
            );
            // qrcodegen returns `DataTooLong` only above v40 capacity;
            // a v1 JSON payload is far below that. Fall through silently
            // if encoding ever fails so the device still pushes a
            // (blank) frame and the operator sees a screen instead of a
            // panic.
            match crate::pairing_overlay::encode_payload(&json) {
                Ok(qr) => {
                    if !self.qr_encoded_logged {
                        log::info!(
                            "QR encoded — JSON {} bytes, modules {}, BD_ADDR {:02X}:{:02X}:{:02X}:{:02X}:{:02X}:{:02X}",
                            json.len(),
                            qr.size(),
                            address[0], address[1], address[2], address[3], address[4], address[5]
                        );
                        self.qr_encoded_logged = true;
                    }
                    crate::pairing_overlay::render_pairing_qr(
                        &qr,
                        &mut self.render_framebuffer,
                    );
                }
                Err(err) => {
                    log::error!(
                        "QR encode failed (json {} bytes): {:?} — operator will see a blank panel",
                        json.len(), err
                    );
                }
            }
        } else {
            // current_qr_payload() returns None only when the state machine
            // is in Operational mode — but we should never reach here in
            // that case because step_frame's gate already excluded it.
            log::warn!("render_pairing_overlay called but pairing.current_qr_payload() is None");
        }
        let render = t_render_start.elapsed();
        let (convert, panel_push) =
            self.display.present_timed(&self.render_framebuffer)?;
        #[cfg(test)]
        {
            self.last_render_path = FramePath::PairingOverlay;
        }
        Ok(FrameResult {
            output: RuntimeFrameOutput::default(),
            geometry_count: 0,
            lit_pixel_count: self.display.framebuffer().lit_pixel_count(),
            route_sync_statuses: Vec::new(),
            phase_timings: PhaseTimings {
                map_query: Duration::ZERO,
                render,
                convert,
                panel_push,
            },
        })
    }

    pub fn ingest_route_sync_chunk(
        &mut self,
        chunk: RouteTransferChunk,
    ) -> Result<Vec<RouteStatusMessage>, RouteSyncTransportError> {
        // Single-bond policy: while we're still in pairing mode, drop
        // route-sync chunks on the floor and bump a diagnostics
        // counter. The companion won't see a status notification, but
        // also can't land a route before bonding — by the time the
        // companion finishes pairing it'll re-send.
        if self.is_in_pairing_mode() {
            self.pairing_dropped_chunks =
                self.pairing_dropped_chunks.saturating_add(1);
            return Ok(Vec::new());
        }
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
        // `NullSettingsStore` is exclusively a test/dev convenience —
        // production builds go through `with_parts_and_settings` with
        // a real `EspIdfSettingsStore` so a freshly-flashed device
        // boots into pairing mode. Tests using `NullSettingsStore`
        // want runtime behavior, so start bonded.
        let mut app = Self::with_parts_and_settings(
            board,
            config,
            map_source,
            display_backend,
            NullSettingsStore,
        )?;
        app.pairing = PairingStateMachine::new_paired(
            [0u8; PERIPHERAL_ADDRESS_LEN],
            [0u8; 16],
        );
        app.persisted_settings.device_paired = true;
        app.persisted_settings.peer_identity = Some([0u8; 16]);
        Ok(app)
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
        // `NullSettingsStore` is a test/dev-only convenience — production
        // boots through `with_parts_and_settings` with a real
        // `EspIdfSettingsStore`. Tests using `with_map_source` want
        // the runtime/render pipeline, not the QR overlay; start
        // bonded so they hit the runtime path.
        let mut app = Self::with_map_source_and_settings(
            board,
            config,
            map_source,
            NullSettingsStore,
        );
        app.pairing = PairingStateMachine::new_paired(
            [0u8; PERIPHERAL_ADDRESS_LEN],
            [0u8; 16],
        );
        app.persisted_settings.device_paired = true;
        app.persisted_settings.peer_identity = Some([0u8; 16]);
        app
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
        let mut app = Self::new(
            board,
            RuntimeConfig {
                viewport_size: board.viewport_size,
                ..RuntimeConfig::default()
            },
        );
        // Test-convenience: start already bonded so tests that don't
        // care about pairing exercise the runtime path. Production
        // boots through `with_parts_and_settings` which respects the
        // persisted bond. Tests that exercise pairing should call
        // `App::with_parts_and_settings` (or `force_unpaired_for_test`)
        // explicitly.
        app.pairing =
            PairingStateMachine::new_paired([0u8; PERIPHERAL_ADDRESS_LEN], [0u8; 16]);
        app.persisted_settings.device_paired = true;
        app.persisted_settings.peer_identity = Some([0u8; 16]);
        app
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
            // Skip pairing mode so the test can exercise the runtime
            // settings round-trip; pairing-mode coverage lives in its
            // own dedicated tests below.
            device_paired: true,
            peer_identity: Some([0u8; 16]),
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
                device_paired: true,
                peer_identity: Some([0u8; 16]),
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

    fn unpaired_app_with_store(
        store: MemorySettingsStore,
    ) -> App<MapSourceBridge, MemoryDisplayBackend, MemorySettingsStore> {
        let board = BoardConfig::default();
        App::with_parts_and_settings(
            board,
            RuntimeConfig {
                viewport_size: board.viewport_size,
                ..RuntimeConfig::default()
            },
            MapSourceBridge::default(),
            MemoryDisplayBackend::default(),
            store,
        )
        .expect("unpaired app with memory settings store")
    }

    #[test]
    fn app_in_pairing_mode_drops_route_writes() {
        // Persisted settings come up unpaired (the default), so the App
        // boots in pairing mode and must refuse to ingest route-sync
        // chunks until the device is bonded.
        let store = MemorySettingsStore::default();
        let mut app = unpaired_app_with_store(store);
        assert!(app.is_in_pairing_mode());

        let message = RouteSyncMessage::Set(RouteSetMessage {
            route: sample_route(1),
        });
        for chunk in chunk_sync_message(&message, "transfer-pairing", 32) {
            let statuses = app
                .ingest_route_sync_chunk(chunk)
                .expect("chunk ingest must not error in pairing mode");
            assert!(
                statuses.is_empty(),
                "pairing-mode ingest must yield no status messages — \
                 the companion has not bonded yet"
            );
        }

        // The runtime must NOT have applied the route — the device is
        // still in pairing mode, so the world buffer just shows the QR.
        assert!(app.pairing_dropped_chunks() > 0);
        assert!(app.route_sync_transport().active_route_revision().is_none());
    }

    fn sample_auth_outcome(success: bool, fail_reason: u8) -> AuthCmplOutcome {
        AuthCmplOutcome {
            success,
            fail_reason,
            peer_addr: [0xAA, 0xBB, 0xCC, 0xDD, 0xEE, 0xFF],
            addr_type: 0x00,
        }
    }

    #[test]
    fn auth_cmpl_success_records_peer_addr() {
        // SMP success comes after the OOB secret has already bonded
        // the device. The auth-cmpl handler just captures the BD_ADDR
        // we'll later push into the controller's whitelist.
        let store = MemorySettingsStore::default();
        let mut app = unpaired_app_with_store(store);
        let _ = app.ingest_pairing_confirm(&[0u8; SECRET_LEN]);
        assert!(app.is_paired_for_test());
        let transition = app.ingest_auth_cmpl(sample_auth_outcome(true, 0));
        assert_eq!(transition, Transition::None);
        let (bd, addr_type) = app.last_bonded_peer_for_test().expect("BD_ADDR captured");
        assert_eq!(bd, [0xAA, 0xBB, 0xCC, 0xDD, 0xEE, 0xFF]);
        assert_eq!(addr_type, 0x00);
    }

    #[test]
    fn auth_cmpl_failure_drops_bond_and_returns_to_unbonded() {
        // The bonded peer forgot us (or its IRK rotated). Bluedroid
        // reports auth_cmpl failure; we drop the bond so the next
        // companion `pairing_request` opens a fresh QR cleanly.
        let store = MemorySettingsStore::new(Some(DeviceSettings {
            device_paired: true,
            peer_identity: Some([0x42; 16]),
            ..DeviceSettings::default()
        }));
        let mut app = unpaired_app_with_store(store.clone());
        assert!(!app.is_in_pairing_mode(), "starts bonded");
        let transition = app.ingest_auth_cmpl(sample_auth_outcome(false, 99));
        assert_eq!(transition, Transition::ClearBondAndPair);
        assert!(app.is_unbonded(), "auth-cmpl failure must drop the bond");
        let persisted = store.shared_value().expect("settings present");
        assert!(!persisted.device_paired, "NVS bond flag must be cleared");
        assert!(persisted.peer_identity.is_none(), "stale peer_identity wiped");
    }

    #[test]
    fn auth_cmpl_failure_while_already_unbonded_is_noop() {
        // Defensive: an auth-cmpl failure that arrives when we never
        // had a bond shouldn't crash the App or churn settings.
        let store = MemorySettingsStore::default();
        let mut app = unpaired_app_with_store(store.clone());
        let baseline = store.shared_value();
        let transition = app.ingest_auth_cmpl(sample_auth_outcome(false, 99));
        assert_eq!(transition, Transition::None);
        assert_eq!(store.shared_value(), baseline, "no NVS write while already unbonded");
    }

    #[test]
    fn pairing_confirm_match_transitions_to_operational_and_persists() {
        // Boot unpaired with a deterministic initial secret so the QR
        // and the inbound pairing-confirm secret line up.
        let store = MemorySettingsStore::default();
        let mut app = unpaired_app_with_store(store.clone());
        assert!(app.is_in_pairing_mode());

        // The initial secret used for `App::default`'s factory is
        // `zero_secret()`, which produces 32 zero bytes. Match it.
        let secret = [0u8; SECRET_LEN];
        let transition = app.ingest_pairing_confirm(&secret);
        assert!(matches!(transition, Transition::Bonded { .. }));
        assert!(!app.is_in_pairing_mode(), "Bonded transition must flip out of pairing mode");

        // Persisted settings must reflect the bond so the next boot
        // does not drop back into pairing mode.
        let persisted = store.shared_value().expect("settings persisted");
        assert!(persisted.device_paired, "device_paired must be set after bond");
        assert!(persisted.peer_identity.is_some(), "peer_identity must be persisted");
    }

    #[test]
    fn unbonded_app_renders_map_until_qr_requested() {
        // The new UX rule: an unbonded device shows the map (the
        // companion-app pairing flow drives the QR display via
        // `request_qr_display`). Boot-time unbonded must NOT short-
        // circuit to the QR overlay — otherwise the user sees a QR
        // every cold boot, which is exactly what we just fixed.
        let store = MemorySettingsStore::default();
        let mut app = unpaired_app_with_store(store);
        let _ = app
            .step_frame(Duration::from_millis(16), None, None)
            .expect("first unbonded frame");
        assert_ne!(
            app.last_render_path(),
            FramePath::PairingOverlay,
            "unbonded boot must render the map, not the QR; the QR is opt-in",
        );
        assert!(!app.is_showing_qr());
    }

    #[test]
    fn request_qr_display_renders_pairing_overlay_for_one_window() {
        let store = MemorySettingsStore::default();
        let mut app = unpaired_app_with_store(store);
        // Companion-side `pairing_request` write hits the firmware →
        // App opens the QR display window.
        app.request_qr_display();
        let _ = app
            .step_frame(Duration::from_millis(16), None, None)
            .expect("frame inside QR window");
        assert_eq!(app.last_render_path(), FramePath::PairingOverlay);
        assert!(app.is_showing_qr());
    }

    #[test]
    fn qr_display_window_expires_back_to_map() {
        let store = MemorySettingsStore::default();
        let mut app = unpaired_app_with_store(store);
        app.request_qr_display();
        // Step well past the QR-display window — App must auto-return
        // to the map even if no pairing-confirm landed.
        let dt = QR_DISPLAY_DURATION + Duration::from_secs(1);
        let _ = app.step_frame(dt, None, None).expect("frame past window");
        assert!(!app.is_showing_qr(), "QR window must auto-expire after timeout");
        assert_ne!(app.last_render_path(), FramePath::PairingOverlay);
    }

    #[test]
    fn successful_pairing_confirm_clears_qr_window_immediately() {
        let store = MemorySettingsStore::default();
        let mut app = unpaired_app_with_store(store);
        app.request_qr_display();
        assert!(app.is_showing_qr());
        let secret = [0u8; SECRET_LEN];
        let transition = app.ingest_pairing_confirm(&secret);
        assert!(matches!(transition, Transition::Bonded { .. }));
        assert!(
            !app.is_showing_qr(),
            "a successful bond must drop the QR overlay immediately, not wait \
             for the deadline",
        );
    }

    #[test]
    fn step_frame_paints_getting_gps_overlay_when_gps_not_acquired_yet() {
        // Black-and-white "GETTING GPS" banner must appear in the
        // center of the framebuffer while the app has not yet seen a
        // real fix. Drives the same code path the device will hit on
        // every cold boot before the NEO-6M reports its first RMC.
        let mut app = App::default();
        app.set_gps_acquired(false);
        let _ = app
            .step_frame(
                Duration::from_millis(16),
                Some(helsinki_fix(24.94210, 0.0)),
                None,
            )
            .expect("frame with overlay");

        let pixels = app.display().framebuffer().pixels();
        let w = app.display().framebuffer().width() as usize;
        let h = app.display().framebuffer().height() as usize;
        // Center pixel sits inside the overlay panel — must be either
        // panel-black or text-white (never an underlying map color).
        let idx = ((h / 2) * w + (w / 2)) * 4;
        let r = pixels[idx];
        let g = pixels[idx + 1];
        let b = pixels[idx + 2];
        let is_panel_or_text =
            (r == 0 && g == 0 && b == 0) || (r == 0xFF && g == 0xFF && b == 0xFF);
        assert!(
            is_panel_or_text,
            "center pixel should be the overlay's panel/text color; got ({r},{g},{b})"
        );

        // After the platform reports a real fix, the overlay clears.
        app.set_gps_acquired(true);
        let _ = app
            .step_frame(
                Duration::from_millis(16),
                Some(helsinki_fix(24.94310, 5.0)),
                None,
            )
            .expect("frame after acquisition");
        let pixels_after = app.display().framebuffer().pixels();
        let r2 = pixels_after[idx];
        let g2 = pixels_after[idx + 1];
        let b2 = pixels_after[idx + 2];
        assert!(
            !(r2 == 0 && g2 == 0 && b2 == 0),
            "center should not still be the panel-black color after acquisition; \
             got ({r2},{g2},{b2}) — the overlay never cleared"
        );
    }

    #[test]
    fn request_qr_display_while_bonded_drops_bond_and_reshows_qr() {
        // Re-pair flow: companion taps "Pair new device" again. The
        // device should drop the prior bond + show a fresh QR.
        let store = MemorySettingsStore::new(Some(DeviceSettings {
            device_paired: true,
            peer_identity: Some([0xAB; 16]),
            ..DeviceSettings::default()
        }));
        let mut app = unpaired_app_with_store(store.clone());
        assert!(!app.is_in_pairing_mode(), "starts bonded");
        app.request_qr_display();
        assert!(app.is_unbonded(), "request_qr_display must drop the prior bond");
        assert!(app.is_showing_qr(), "and show the QR");
        let persisted = store.shared_value().expect("settings present");
        assert!(!persisted.device_paired, "NVS bond flag must be cleared");
        assert!(persisted.peer_identity.is_none(), "stale peer_identity wiped");
    }
}
