use runtime_core::api::RuntimeConfig;
use runtime_core::map::MapSource;

use crate::app::{App, AppBuildError};
use crate::board_config::{BoardConfig, DisplayConfig, TouchControllerConfig};
use crate::display::{DisplayBackend, DisplayError, MemoryDisplayBackend};
use crate::framebuffer::Framebuffer;
use crate::gps::{GpsDiagnostics, GpsError, GpsInput, GpsProvider, NullGpsProvider};
use crate::map_source::MapSourceBridge;
use crate::platform::{NullTouchSource, RouteSyncIo, RuntimePlatform, SystemFrameClock};
use crate::settings::{DefaultSettingsStore, SettingsStore, default_settings_store};
use crate::touch::{Gt9271Transport, PollingTouchSource, TouchError};

const GT9271_PRODUCT_ID_REGISTER: u16 = 0x8140;
const GT9271_STATUS_REGISTER: u16 = 0x814e;
const GT9271_FIRST_POINT_REGISTER: u16 = 0x814f;
const GT9271_READY_MASK: u8 = 0x80;
const GT9271_TOUCH_COUNT_MASK: u8 = 0x0f;
const GT9271_CONTACT_RECORD_LEN: usize = 8;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EspIdfError {
    Io(String),
    Unsupported(String),
}

pub trait EspIdfI2cBus: std::fmt::Debug {
    fn write(&mut self, address: u16, payload: &[u8]) -> Result<(), EspIdfError>;
    fn write_read(
        &mut self,
        address: u16,
        tx_payload: &[u8],
        rx_payload: &mut [u8],
    ) -> Result<(), EspIdfError>;
}

pub trait EspIdfOutputPin: std::fmt::Debug {
    fn set_low(&mut self) -> Result<(), EspIdfError>;
    fn set_high(&mut self) -> Result<(), EspIdfError>;
}

pub trait EspIdfDelay: std::fmt::Debug {
    fn delay_ms(&mut self, ms: u32);
}

pub trait EspIdfPanel: std::fmt::Debug {
    fn initialize(&mut self, config: DisplayConfig) -> Result<(), EspIdfError>;
    fn present(
        &mut self,
        pixels: &[u8],
        width: u32,
        height: u32,
        config: DisplayConfig,
    ) -> Result<(), EspIdfError>;
}

pub trait EspIdfGpsSerial: std::fmt::Debug {
    fn read_sentence(&mut self) -> Result<Option<String>, EspIdfError>;

    /// Number of raw bytes pulled off the UART RX FIFO since the
    /// transport was constructed. Surfaces on the "GETTING GPS"
    /// overlay so a field operator can tell whether the wiring is
    /// even passing electrons. Default 0 for transports that don't
    /// instrument this (e.g. test fixtures); the real
    /// [`crate::gps_uart::UartGpsSerial`] overrides it.
    fn bytes_seen(&self) -> u64 {
        0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct NoopDelay;

impl EspIdfDelay for NoopDelay {
    fn delay_ms(&mut self, _ms: u32) {}
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct NoopPin;

impl EspIdfOutputPin for NoopPin {
    fn set_low(&mut self) -> Result<(), EspIdfError> {
        Ok(())
    }

    fn set_high(&mut self) -> Result<(), EspIdfError> {
        Ok(())
    }
}

#[derive(Debug, Clone)]
pub struct EspIdfGt9271Transport<B, R, I, D>
where
    B: EspIdfI2cBus,
    R: EspIdfOutputPin,
    I: EspIdfOutputPin,
    D: EspIdfDelay,
{
    address: u16,
    bus: B,
    rst_pin: Option<R>,
    int_pin: Option<I>,
    delay: D,
}

impl<B, R, I, D> EspIdfGt9271Transport<B, R, I, D>
where
    B: EspIdfI2cBus,
    R: EspIdfOutputPin,
    I: EspIdfOutputPin,
    D: EspIdfDelay,
{
    pub fn new(address: u16, bus: B, rst_pin: Option<R>, int_pin: Option<I>, delay: D) -> Self {
        Self {
            address,
            bus,
            rst_pin,
            int_pin,
            delay,
        }
    }

    pub fn bus(&self) -> &B {
        &self.bus
    }
}

impl<B, R, I, D> Gt9271Transport for EspIdfGt9271Transport<B, R, I, D>
where
    B: EspIdfI2cBus,
    R: EspIdfOutputPin,
    I: EspIdfOutputPin,
    D: EspIdfDelay,
{
    fn reset(&mut self, config: TouchControllerConfig) -> Result<(), TouchError> {
        if let Some(rst_pin) = &mut self.rst_pin {
            rst_pin
                .set_low()
                .map_err(|error| TouchError::Controller(format!("{error:?}")))?;
        }
        if let Some(int_pin) = &mut self.int_pin {
            if config.address == 0x5d {
                int_pin
                    .set_high()
                    .map_err(|error| TouchError::Controller(format!("{error:?}")))?;
            } else {
                int_pin
                    .set_low()
                    .map_err(|error| TouchError::Controller(format!("{error:?}")))?;
            }
        }
        self.delay.delay_ms(10);
        if let Some(rst_pin) = &mut self.rst_pin {
            rst_pin
                .set_high()
                .map_err(|error| TouchError::Controller(format!("{error:?}")))?;
        }
        self.delay.delay_ms(50);
        Ok(())
    }

    fn read_product_id(&mut self) -> Result<[u8; 4], TouchError> {
        let mut product_id = [0_u8; 4];
        self.bus
            .write_read(
                self.address,
                &GT9271_PRODUCT_ID_REGISTER.to_be_bytes(),
                &mut product_id,
            )
            .map_err(|error| TouchError::Controller(format!("{error:?}")))?;
        Ok(product_id)
    }

    fn read_touch_report(&mut self, config: TouchControllerConfig) -> Result<Vec<u8>, TouchError> {
        let mut status = [0_u8; 1];
        self.bus
            .write_read(
                config.address,
                &GT9271_STATUS_REGISTER.to_be_bytes(),
                &mut status,
            )
            .map_err(|error| TouchError::Controller(format!("{error:?}")))?;

        let touch_count = usize::from(status[0] & GT9271_TOUCH_COUNT_MASK);
        let mut report = vec![status[0]];
        if status[0] & GT9271_READY_MASK != 0 && touch_count > 0 {
            let mut points = vec![0_u8; touch_count * GT9271_CONTACT_RECORD_LEN];
            self.bus
                .write_read(
                    config.address,
                    &GT9271_FIRST_POINT_REGISTER.to_be_bytes(),
                    &mut points,
                )
                .map_err(|error| TouchError::Controller(format!("{error:?}")))?;
            report.extend_from_slice(&points);
        }
        self.bus
            .write(
                config.address,
                &[
                    GT9271_STATUS_REGISTER.to_be_bytes()[0],
                    GT9271_STATUS_REGISTER.to_be_bytes()[1],
                    0,
                ],
            )
            .map_err(|error| TouchError::Controller(format!("{error:?}")))?;
        Ok(report)
    }
}

#[derive(Debug, Clone)]
pub struct EspIdfDisplayBackend<P>
where
    P: EspIdfPanel,
{
    panel: P,
    config: Option<DisplayConfig>,
}

impl<P> EspIdfDisplayBackend<P>
where
    P: EspIdfPanel,
{
    pub fn new(panel: P) -> Self {
        Self {
            panel,
            config: None,
        }
    }

    pub fn panel(&self) -> &P {
        &self.panel
    }
}

impl<P> DisplayBackend for EspIdfDisplayBackend<P>
where
    P: EspIdfPanel,
{
    fn initialize(&mut self, config: DisplayConfig) -> Result<(), DisplayError> {
        self.panel
            .initialize(config)
            .map_err(|error| DisplayError::Backend(format!("{error:?}")))?;
        self.config = Some(config);
        Ok(())
    }

    fn present(&mut self, framebuffer: &Framebuffer) -> Result<(), DisplayError> {
        let config = self.config.ok_or_else(|| {
            DisplayError::Backend("panel backend used before initialize".to_owned())
        })?;
        self.panel
            .present(
                framebuffer.panel_pixels(),
                framebuffer.width(),
                framebuffer.height(),
                config,
            )
            .map_err(|error| DisplayError::Backend(format!("{error:?}")))
    }
}

/// Periodic-status log cadence while the receiver is still searching
/// for satellites — fast enough that the operator can see "bytes
/// flowing yet?" within a few seconds, slow enough that the console
/// isn't drowned by it.
const GPS_STATUS_LOG_INTERVAL_SEARCHING: std::time::Duration =
    std::time::Duration::from_secs(5);

/// Periodic-status log cadence once locked. We don't need a per-5 s
/// reminder that the GPS is still working; once a minute is plenty
/// for "I'm still here" reassurance.
const GPS_STATUS_LOG_INTERVAL_LOCKED: std::time::Duration =
    std::time::Duration::from_secs(60);

/// Minimum gap between successive per-fix INFO lines after the very
/// first one. Without this we'd log 60 lines per minute at the 1 Hz
/// NMEA cadence — useless noise once the operator has confirmed
/// acquisition.
const GPS_FIX_LOG_INTERVAL: std::time::Duration = std::time::Duration::from_secs(30);

#[derive(Debug, Clone)]
pub struct EspIdfGpsProvider<S>
where
    S: EspIdfGpsSerial,
{
    serial: S,
    sentences_seen: u64,
    fixes_seen: u64,
    last_sentence_kind: Option<String>,
    last_status_log: Option<std::time::Instant>,
    /// Wallclock instant of the most recent successful RMC parse.
    /// Used both for log throttling (so we don't repeat per-fix INFO
    /// lines at 1 Hz) and to fill `GpsDiagnostics::last_fix_age_ms`
    /// (so the platform layer can detect signal loss after lock and
    /// flip the overlay back on).
    last_fix_at: Option<std::time::Instant>,
    /// Wallclock instant of the most recent per-fix INFO log line —
    /// driver of the 30 s cadence in `GPS_FIX_LOG_INTERVAL`.
    last_fix_log: Option<std::time::Instant>,
}

impl<S> EspIdfGpsProvider<S>
where
    S: EspIdfGpsSerial,
{
    pub fn new(serial: S) -> Self {
        Self {
            serial,
            sentences_seen: 0,
            fixes_seen: 0,
            last_sentence_kind: None,
            last_status_log: None,
            last_fix_at: None,
            last_fix_log: None,
        }
    }

    fn maybe_log_status(&mut self) {
        let now = std::time::Instant::now();
        let interval = if self.fixes_seen == 0 {
            GPS_STATUS_LOG_INTERVAL_SEARCHING
        } else {
            GPS_STATUS_LOG_INTERVAL_LOCKED
        };
        let due = self
            .last_status_log
            .map(|last| now.duration_since(last) >= interval)
            .unwrap_or(true);
        if !due {
            return;
        }
        self.last_status_log = Some(now);
        let kind = self
            .last_sentence_kind
            .as_deref()
            .unwrap_or("(none yet)");
        if self.fixes_seen == 0 {
            log::info!(
                "gps: waiting for fix — sentences_seen={} last_kind={} (NEO-6M cold start can take 30 s … several minutes; outdoors with sky view recommended)",
                self.sentences_seen,
                kind,
            );
        } else {
            log::info!(
                "gps: sentences_seen={} fixes_seen={} last_kind={}",
                self.sentences_seen,
                self.fixes_seen,
                kind,
            );
        }
    }
}

impl<S> GpsProvider for EspIdfGpsProvider<S>
where
    S: EspIdfGpsSerial,
{
    fn diagnostics_summary(&self) -> Option<GpsDiagnostics> {
        let last_fix_age_ms = self.last_fix_at.map(|at| {
            std::time::Instant::now()
                .saturating_duration_since(at)
                .as_millis()
                .min(u32::MAX as u128) as u32
        });
        Some(GpsDiagnostics {
            bytes_seen: self.serial.bytes_seen(),
            sentences_seen: self.sentences_seen,
            fixes_seen: self.fixes_seen,
            last_fix_age_ms,
        })
    }

    fn poll(&mut self) -> Result<Option<GpsInput>, GpsError> {
        let sentence = self
            .serial
            .read_sentence()
            .map_err(|error| GpsError::Provider(format!("{error:?}")))?;

        self.maybe_log_status();

        let Some(sentence) = sentence else {
            return Ok(None);
        };

        self.sentences_seen += 1;
        // Cache the talker+type prefix (e.g. "GPRMC", "GPGSV") for the
        // periodic status log so the operator can tell at a glance
        // whether they're seeing fix-bearing sentences (RMC/GGA) or
        // only satellite-search ones (GSV/GSA).
        if let Some(kind) = nmea_kind(&sentence) {
            self.last_sentence_kind = Some(kind);
        }
        log::debug!("gps nmea: {sentence}");

        match parse_rmc_sentence(&sentence) {
            Some(fix) => {
                self.fixes_seen += 1;
                let now = std::time::Instant::now();
                self.last_fix_at = Some(now);
                // Per-fix INFO log: emit the very first fix immediately
                // (so the operator sees acquisition succeed in real
                // time), then throttle subsequent fixes to once every
                // `GPS_FIX_LOG_INTERVAL` so the steady-state 1 Hz
                // stream doesn't drown the console.
                let due = self
                    .last_fix_log
                    .map(|last| now.duration_since(last) >= GPS_FIX_LOG_INTERVAL)
                    .unwrap_or(true);
                if due {
                    self.last_fix_log = Some(now);
                    log::info!(
                        "gps fix: lat={:.6} lon={:.6} speed_mps={:.2} course_rad={:?}",
                        fix.lat_deg,
                        fix.lon_deg,
                        fix.speed_mps,
                        fix.course_rad,
                    );
                }
                Ok(Some(fix))
            }
            None => Ok(None),
        }
    }
}

/// Strips the `$` and pulls the talker+type field (`GPRMC`, `GNGGA`, …)
/// out of an NMEA-0183 sentence. Returns `None` for malformed input so
/// the caller falls back to its previous label.
fn nmea_kind(sentence: &str) -> Option<String> {
    let payload = sentence.trim().strip_prefix('$')?;
    let head = payload.split(',').next()?;
    if head.is_empty() {
        None
    } else {
        Some(head.to_owned())
    }
}

pub type DevicePlatform<T, R, I, D, G, S, P, U = DefaultSettingsStore> = RuntimePlatform<
    PollingTouchSource<EspIdfGt9271Transport<T, R, I, D>>,
    G,
    SystemFrameClock,
    S,
    EspIdfDisplayBackend<P>,
    U,
>;

pub type DefaultDevicePlatform<T, R, I, D, G, P, U = DefaultSettingsStore> =
    DevicePlatform<T, R, I, D, G, MapSourceBridge, P, U>;

pub type DevicePlatformResult<T, R, I, D, G, S, P, U = DefaultSettingsStore> =
    Result<DevicePlatform<T, R, I, D, G, S, P, U>, AppBuildError>;

pub type DefaultDevicePlatformResult<T, R, I, D, G, P, U = DefaultSettingsStore> =
    Result<DefaultDevicePlatform<T, R, I, D, G, P, U>, AppBuildError>;

pub type HeadlessRouteSyncDevicePlatform<Q, U = DefaultSettingsStore> = RuntimePlatform<
    NullTouchSource,
    NullGpsProvider,
    SystemFrameClock,
    MapSourceBridge,
    MemoryDisplayBackend,
    U,
    Q,
>;

pub type HeadlessRouteSyncDevicePlatformResult<Q, U = DefaultSettingsStore> =
    Result<HeadlessRouteSyncDevicePlatform<Q, U>, AppBuildError>;

pub fn build_headless_route_sync_platform_with_settings<Q, U>(
    board: BoardConfig,
    speed_unit_store: U,
    route_sync: Q,
) -> HeadlessRouteSyncDevicePlatformResult<Q, U>
where
    Q: RouteSyncIo,
    U: SettingsStore,
{
    let runtime_config = RuntimeConfig {
        viewport_size: board.viewport_size,
        ..RuntimeConfig::default()
    };
    let app = App::with_parts_and_settings(
        board,
        runtime_config,
        MapSourceBridge::default(),
        MemoryDisplayBackend::default(),
        speed_unit_store,
    )?;
    Ok(RuntimePlatform::with_route_sync(
        app,
        NullTouchSource,
        NullGpsProvider,
        SystemFrameClock::new(board.frame_interval),
        route_sync,
    ))
}

pub fn build_headless_route_sync_platform<Q>(
    board: BoardConfig,
    route_sync: Q,
) -> HeadlessRouteSyncDevicePlatformResult<Q>
where
    Q: RouteSyncIo,
{
    build_headless_route_sync_platform_with_settings(board, default_settings_store()?, route_sync)
}

pub fn build_device_platform_with_route_sync_and_settings<T, R, I, D, P, G, S, U, Q>(
    board: BoardConfig,
    map_source: S,
    touch_transport: EspIdfGt9271Transport<T, R, I, D>,
    display_backend: EspIdfDisplayBackend<P>,
    gps_provider: G,
    speed_unit_store: U,
    route_sync: Q,
) -> Result<
    RuntimePlatform<
        PollingTouchSource<EspIdfGt9271Transport<T, R, I, D>>,
        G,
        SystemFrameClock,
        S,
        EspIdfDisplayBackend<P>,
        U,
        Q,
    >,
    AppBuildError,
>
where
    T: EspIdfI2cBus,
    R: EspIdfOutputPin,
    I: EspIdfOutputPin,
    D: EspIdfDelay,
    P: EspIdfPanel,
    G: GpsProvider,
    S: MapSource,
    U: SettingsStore,
    Q: RouteSyncIo,
{
    let runtime_config = RuntimeConfig {
        viewport_size: board.viewport_size,
        ..RuntimeConfig::default()
    };
    let touch_source = PollingTouchSource::new(board.touch, touch_transport);
    let app = App::with_parts_and_settings(
        board,
        runtime_config,
        map_source,
        display_backend,
        speed_unit_store,
    )?;
    Ok(RuntimePlatform::with_route_sync(
        app,
        touch_source,
        gps_provider,
        SystemFrameClock::new(board.frame_interval),
        route_sync,
    ))
}

pub fn build_device_platform_with_route_sync<T, R, I, D, P, G, S, Q>(
    board: BoardConfig,
    map_source: S,
    touch_transport: EspIdfGt9271Transport<T, R, I, D>,
    display_backend: EspIdfDisplayBackend<P>,
    gps_provider: G,
    route_sync: Q,
) -> Result<
    RuntimePlatform<
        PollingTouchSource<EspIdfGt9271Transport<T, R, I, D>>,
        G,
        SystemFrameClock,
        S,
        EspIdfDisplayBackend<P>,
        DefaultSettingsStore,
        Q,
    >,
    AppBuildError,
>
where
    T: EspIdfI2cBus,
    R: EspIdfOutputPin,
    I: EspIdfOutputPin,
    D: EspIdfDelay,
    P: EspIdfPanel,
    G: GpsProvider,
    S: MapSource,
    Q: RouteSyncIo,
{
    build_device_platform_with_route_sync_and_settings(
        board,
        map_source,
        touch_transport,
        display_backend,
        gps_provider,
        default_settings_store()?,
        route_sync,
    )
}

pub fn build_device_platform_with_settings<T, R, I, D, P, G, S, U>(
    board: BoardConfig,
    map_source: S,
    touch_transport: EspIdfGt9271Transport<T, R, I, D>,
    display_backend: EspIdfDisplayBackend<P>,
    gps_provider: G,
    speed_unit_store: U,
) -> DevicePlatformResult<T, R, I, D, G, S, P, U>
where
    T: EspIdfI2cBus,
    R: EspIdfOutputPin,
    I: EspIdfOutputPin,
    D: EspIdfDelay,
    P: EspIdfPanel,
    G: GpsProvider,
    S: MapSource,
    U: SettingsStore,
{
    let runtime_config = RuntimeConfig {
        viewport_size: board.viewport_size,
        ..RuntimeConfig::default()
    };
    let touch_source = PollingTouchSource::new(board.touch, touch_transport);
    let app = App::with_parts_and_settings(
        board,
        runtime_config,
        map_source,
        display_backend,
        speed_unit_store,
    )?;
    Ok(RuntimePlatform::new(
        app,
        touch_source,
        gps_provider,
        SystemFrameClock::new(board.frame_interval),
    ))
}

pub fn build_device_platform<T, R, I, D, P, G, S>(
    board: BoardConfig,
    map_source: S,
    touch_transport: EspIdfGt9271Transport<T, R, I, D>,
    display_backend: EspIdfDisplayBackend<P>,
    gps_provider: G,
) -> DevicePlatformResult<T, R, I, D, G, S, P>
where
    T: EspIdfI2cBus,
    R: EspIdfOutputPin,
    I: EspIdfOutputPin,
    D: EspIdfDelay,
    P: EspIdfPanel,
    G: GpsProvider,
    S: MapSource,
{
    build_device_platform_with_settings(
        board,
        map_source,
        touch_transport,
        display_backend,
        gps_provider,
        default_settings_store()?,
    )
}

pub fn build_default_device_platform<T, R, I, D, P, G>(
    board: BoardConfig,
    touch_transport: EspIdfGt9271Transport<T, R, I, D>,
    display_backend: EspIdfDisplayBackend<P>,
    gps_provider: G,
) -> DefaultDevicePlatformResult<T, R, I, D, G, P>
where
    T: EspIdfI2cBus,
    R: EspIdfOutputPin,
    I: EspIdfOutputPin,
    D: EspIdfDelay,
    P: EspIdfPanel,
    G: GpsProvider,
{
    build_device_platform(
        board,
        MapSourceBridge::default(),
        touch_transport,
        display_backend,
        gps_provider,
    )
}

#[cfg(all(
    target_os = "espidf",
    not(any(esp32s2, esp32p4)),
    esp_idf_bt_enabled,
    esp_idf_bt_bluedroid_enabled
))]
pub fn run_device_main() -> Result<(), String> {
    use std::thread;

    use esp_idf_svc::bt::{Ble, BtDriver};
    use esp_idf_svc::hal::peripherals::Peripherals;
    use esp_idf_svc::log::EspLogger;
    use esp_idf_svc::sys;

    sys::link_patches();
    EspLogger::initialize_default();

    let peripherals = Peripherals::take()
        .map_err(|error| format!("failed to take ESP-IDF peripherals: {error:?}"))?;
    let board = BoardConfig::default();
    let bt_driver = BtDriver::<Ble>::new(peripherals.modem, None)
        .map_err(|error| format!("failed to initialize Bluetooth controller: {error:?}"))?;
    let route_sync = crate::esp_idf_ble::EspIdfBleRouteSyncIo::new(bt_driver)
        .map_err(|error| format!("failed to initialize BLE GATT route-sync service: {error:?}"))?;
    let mut platform = build_headless_route_sync_platform(board, route_sync)
        .map_err(|error| format!("failed to build headless BLE route-sync platform: {error:?}"))?;

    println!(
        "ESP route-sync BLE service online: {} / {} / {}",
        crate::esp_idf_ble::gatt_service_summary().0,
        crate::esp_idf_ble::gatt_service_summary().1,
        crate::esp_idf_ble::gatt_service_summary().2
    );

    loop {
        platform
            .run_frame()
            .map_err(|error| format!("device frame failed: {error:?}"))?;
        thread::sleep(board.frame_interval);
    }
}

/// Tries to load the map from `/sdcard/map.svm`. Returns `None` if no card
/// is inserted, the file is missing, the buffer doesn't fit in PSRAM, or any
/// other error — all non-fatal so the caller can fall back to the flash
/// partition. The C side (`components/sdmap/sdmap.c`) mounts read-only and
/// never writes or formats the card.
#[cfg(all(target_os = "espidf", esp32p4))]
fn try_load_map_from_sd() -> Option<map_runtime::EmbeddedMapSource> {
    use esp_idf_svc::sys;

    let mut size: usize = 0;
    let buf = unsafe { sys::sdmap_load(&mut size) };
    if buf.is_null() {
        return None;
    }

    let result = unsafe {
        let bytes = core::slice::from_raw_parts(buf, size);
        map_runtime::EmbeddedMapSource::from_svm_bytes(bytes)
    };

    unsafe { sys::sdmap_free(buf) };

    match result {
        Ok(map) => {
            log::info!("loaded map from SD card ({} bytes)", size);
            Some(map)
        }
        Err(e) => {
            log::warn!("SD map parse failed: {e} — falling back to flash");
            None
        }
    }
}

/// Reads the SVM map file from the "map_data" flash partition.
///
/// The partition holds the city-small.svm binary starting at its first byte.
/// The map is memory-mapped (zero-copy) for parsing, then unmapped; the parsed
/// `EmbeddedMapSource` keeps segments in PSRAM via its internal Vec.
#[cfg(all(target_os = "espidf", esp32p4))]
fn load_map_from_partition() -> Result<map_runtime::EmbeddedMapSource, String> {
    use esp_idf_svc::sys;

    // Partition type DATA = 0x01, custom subtype 0x80
    const DATA: u32 = 0x01;
    const SUBTYPE: u32 = 0x80;

    unsafe {
        let partition = sys::esp_partition_find_first(
            DATA,
            SUBTYPE,
            b"map_data\0".as_ptr() as *const core::ffi::c_char,
        );
        if partition.is_null() {
            return Err(
                "map_data partition not found — make sure city-small.svm \
                 is bundled into the flash image at offset 0x1010000"
                    .to_owned(),
            );
        }

        let part_size = (*partition).size as usize;
        log::info!("map_data partition: {} MB", part_size / 1_048_576);

        // Memory-map the partition for zero-copy parsing.
        // SPI_FLASH_MMAP_DATA = 0; segments are copied into PSRAM Vec during
        // from_svm_bytes, so we unmap immediately after.
        let mut mmap_ptr: *const core::ffi::c_void = core::ptr::null();
        let mut mmap_handle: sys::spi_flash_mmap_handle_t = 0;

        let err = sys::esp_partition_mmap(
            partition,
            0,
            part_size,
            0, // SPI_FLASH_MMAP_DATA
            &mut mmap_ptr,
            &mut mmap_handle,
        );
        if err != 0 {
            return Err(format!("esp_partition_mmap failed: err={err}"));
        }

        let bytes = core::slice::from_raw_parts(mmap_ptr as *const u8, part_size);
        let result = map_runtime::EmbeddedMapSource::from_svm_bytes(bytes)
            .map_err(|e| format!("map SVM parse failed: {e}"));

        sys::spi_flash_munmap(mmap_handle);
        result
    }
}

// ESP32-P4 bring-up entrypoint. The P4 has no on-chip radio, so the BLE
// controller runs on the on-board ESP32-C6 over the hosted-SDIO HCI link
// (see components/hosted_ble). After hosted_ble_route_sync_start() returns,
// the standard BLE GATT service from `docs/ble-route-sync-contract.md` is
// advertising and the same RouteSyncTransport that backs the host tests
// is plumbed straight into the runtime loop.
#[cfg(all(target_os = "espidf", esp32p4))]
pub fn run_device_main() -> Result<(), String> {
    use std::thread;

    use esp_idf_svc::log::EspLogger;
    use esp_idf_svc::sys;
    use runtime_core::api::RuntimeConfig;

    use crate::app::App;
    use crate::gps::{GpsInput, SeedThenRealGpsProvider};
    use crate::gps_uart::UartGpsSerial;
    use crate::hosted_ble::HostedBleRouteSyncIo;
    use crate::map_source::MapSourceBridge;
    use crate::mipi_dsi::{
        self, MipiDsiPanel, PanelGpios, waveshare_3p4c_config,
    };
    use crate::platform::{RuntimePlatform, SystemFrameClock};
    use crate::touch_gt911::Gt911TouchSource;
    use crate::settings::default_settings_store;

    sys::link_patches();
    EspLogger::initialize_default();

    // Report PSRAM / internal heap on boot so the operator can confirm the
    // SPI RAM initialized and is usable for the framebuffer. Large (>16KB)
    // allocations route to PSRAM via CONFIG_SPIRAM_USE_MALLOC + ALWAYSINTERNAL
    // thresholds; this log lets us verify at runtime.
    unsafe {
        let psram = sys::heap_caps_get_free_size(sys::MALLOC_CAP_SPIRAM);
        let internal = sys::heap_caps_get_free_size(sys::MALLOC_CAP_INTERNAL);
        log::info!(
            "esp32p4 heap: internal_free={} KB, psram_free={} KB",
            internal / 1024,
            psram / 1024,
        );
    }

    // Ground-truth PSRAM bandwidth before we extrapolate render-pipeline
    // budgets from theoretical numbers. Two `Vec<u8>` of 4 MiB each go
    // straight to PSRAM (above the IDF's `CONFIG_SPIRAM_MALLOC_ALWAYSINTERNAL`
    // threshold). `copy_from_slice` lowers to memcpy, exercising both a
    // PSRAM read stream and a PSRAM write stream — same access pattern as
    // a per-frame framebuffer copy. The reported MB/s number is what we
    // can actually expect for `present_from_render` + `draw_bitmap`.
    {
        const PROBE_BYTES: usize = 4 * 1024 * 1024;
        let src: Vec<u8> = vec![0xAB; PROBE_BYTES];
        let mut dst: Vec<u8> = vec![0; PROBE_BYTES];
        let t0 = std::time::Instant::now();
        dst.copy_from_slice(&src);
        let elapsed = t0.elapsed();
        let mb_per_s = (PROBE_BYTES as f64 / 1_048_576.0) / elapsed.as_secs_f64();
        log::info!(
            "psram probe: 4 MiB memcpy in {} us → {:.1} MiB/s effective bandwidth",
            elapsed.as_micros(),
            mb_per_s,
        );
        // Drop both before we allocate the framebuffer; we don't want
        // 8 MiB sitting around for the rest of the boot.
        drop(dst);
        drop(src);
    }

    let board = BoardConfig::default();

    // Try the SD card first (read-only `/sdcard/map.svm`); fall back to the
    // on-flash `map_data` partition if no card is inserted or the file is
    // missing. SD lets users carry a larger map without reflashing, but the
    // file must still fit in PSRAM (≈ 24 MB / ≈ 1 M segments after parse).
    let map = match try_load_map_from_sd() {
        Some(map) => map,
        None => load_map_from_partition()?,
    };
    let (map_min_x, map_max_x, map_min_y, map_max_y) = map.bounds_world();
    let center_lat_lon = map.center_lat_lon();
    log::info!(
        "map: {} segments, bounds_m=({:.0}..{:.0}, {:.0}..{:.0}), center={:?}",
        map.segment_count(),
        map_min_x,
        map_max_x,
        map_min_y,
        map_max_y,
        center_lat_lon,
    );

    // Build the same headless RuntimePlatform the original helper produces,
    // but swap the GPS provider for a fixed fix at the map center so the
    // render pipeline actually has geometry to draw until real GPS / touch
    // hardware is wired.
    // Bring up the Waveshare 3.4″ round 800×800 MIPI-DSI panel
    // (JD9365 driver IC, 2 lanes @ 1500 Mbps, 80 MHz DPI clock).
    //
    // Pin map and timing values come straight from the official
    // `waveshare/esp32_p4_wifi6_touch_lcd_xc` BSP — confirmed against the
    // kit's schematic. The JD9365 init sequence (~200 DCS commands) is
    // shipped via the `espressif/esp_lcd_jd9365` managed component which
    // is pulled in through esp-idf-sys metadata in firmware/Cargo.toml.
    //
    // Order:
    //   1. LDO3 → MIPI-DSI PHY power (BSP: `BSP_MIPI_DSI_PHY_PWR_LDO_CHAN`).
    //   2. Backlight GPIO26 high.
    //   3. Construct `MipiDsiPanel` — opens DSI bus, DBI IO, builds the
    //      JD9365 vendor config, calls `esp_lcd_new_panel_jd9365` (which
    //      handles reset GPIO27 + transmits the init sequence + creates
    //      the DPI panel), then `esp_lcd_panel_reset` and
    //      `esp_lcd_panel_init`. Returns ready-to-use.
    //   4. Wrap in `EspIdfDisplayBackend` so the runtime feeds frames
    //      through the trait surface.
    let gpios = PanelGpios::WAVESHARE_3P4C;
    let _phy_ldo = mipi_dsi::acquire_mipi_dsi_phy_power()
        .map_err(|error| format!("failed to power MIPI-DSI PHY: {error:?}"))?;

    if let Some(bl) = gpios.backlight {
        mipi_dsi::enable_backlight(bl)
            .map_err(|error| format!("failed to enable panel backlight: {error:?}"))?;
    }
    let panel = MipiDsiPanel::new(waveshare_3p4c_config())
        .map_err(|error| format!("failed to bring up MIPI-DSI panel: {error:?}"))?;
    let display_backend = EspIdfDisplayBackend::new(panel);
    log::info!(
        "mipi-dsi panel: Waveshare 3.4C JD9365, 800x800, 2 lanes @ 1500 Mbps, DPI 80 MHz"
    );

    let runtime_config = RuntimeConfig {
        viewport_size: board.viewport_size,
        ..RuntimeConfig::default()
    };
    let map_source = MapSourceBridge::new(map);
    let app = App::with_parts_and_settings(
        board,
        runtime_config,
        map_source,
        display_backend,
        default_settings_store()
            .map_err(|error| format!("failed to open settings store: {error:?}"))?,
    )
    .map_err(|error| format!("failed to build P4 app with panel: {error:?}"))?;

    // Bring up the NEO-6M on UART1 at 9600 baud. Until the module
    // reports its first valid RMC fix — outdoor cold start is
    // typically 30 s and can stretch to several minutes —
    // `SeedThenRealGpsProvider` parks the camera on the embedded map's
    // own centroid (Helsinki area for the bundled `city-small.svm`)
    // instead of (0, 0) "Gulf of Guinea". The "GETTING GPS" overlay
    // hides the rider marker during this window. Once a real fix
    // arrives, the seed stops being substituted and the rider follows
    // GPS as normal.
    let (seed_lat, seed_lon) = center_lat_lon.unwrap_or((60.1699, 24.9384));
    let real_gps_serial = UartGpsSerial::new_neo6m_uart1().map_err(|error| {
        format!("failed to bring up GPS UART (NEO-6M @ default GPIOs): {error:?}")
    })?;
    let gps = SeedThenRealGpsProvider::new(
        GpsInput {
            lat_deg: seed_lat,
            lon_deg: seed_lon,
            speed_mps: 0.0,
            course_rad: None,
            horizontal_accuracy_m: Some(5.0),
        },
        EspIdfGpsProvider::new(real_gps_serial),
    );
    log::info!(
        "gps: NEO-6M wired to UART1 @ 9600 baud (MCU TX = GPIO{}, MCU RX = GPIO{}); camera seeded at map centroid lat={:.4} lon={:.4} until first real fix",
        crate::gps_uart::WAVESHARE_3P4C_GPS_TX_GPIO,
        crate::gps_uart::WAVESHARE_3P4C_GPS_RX_GPIO,
        seed_lat,
        seed_lon,
    );

    // Bring up the GT911 capacitive touch controller on the same I2C bus
    // the panel CH422G shares (SDA=GPIO7, SCL=GPIO8 @ 400 kHz). The 3.4C
    // schematic ties INT to NC, so the driver runs in polled mode — we
    // hit `esp_lcd_touch_read_data` once per frame inside the runtime.
    let touch_source = Gt911TouchSource::new(board.touch)
        .map_err(|error| format!("failed to bring up GT911 touch: {error:?}"))?;

    // Bring up Bluedroid against the on-board C6 radio and start the
    // route-sync GATT service. Companions (iOS / Android) discover the
    // service by UUID, write chunked route packages to the chunk
    // characteristic, and subscribe to status / reroute notifications on
    // the event characteristic. See `docs/ble-route-sync-contract.md`.
    //
    // If the C6 isn't running matching esp_hosted slave firmware,
    // `start_or_fallback` logs a warning and returns an `Inactive`
    // transport; the runtime keeps rendering / GPS / touch alive while
    // BLE stays offline.
    let route_sync = HostedBleRouteSyncIo::start_or_fallback();
    if route_sync.is_active() {
        let (service_uuid, chunk_uuid, event_uuid) =
            crate::esp_idf_ble::gatt_service_summary();
        log::info!(
            "ble route-sync online via ESP32-C6: service={} chunk_w={} event_n={} \
             pairing_confirm=8d0f3f30-7b4d-4f7c-8b24-2f8e7e4e1004 \
             pairing_request=8d0f3f30-7b4d-4f7c-8b24-2f8e7e4e1005",
            service_uuid,
            chunk_uuid,
            event_uuid,
        );
    }

    let mut platform = RuntimePlatform::with_route_sync(
        app,
        touch_source,
        gps,
        SystemFrameClock::new(board.frame_interval),
        route_sync,
    );

    log::info!(
        "esp32p4 bring-up: viewport={}x{}, frame_interval_ms={}",
        board.viewport_size.width_px,
        board.viewport_size.height_px,
        board.frame_interval.as_millis()
    );

    let mut last_log = std::time::Instant::now();
    let mut frames_since_last_log: u32 = 0;
    let mut frame_work_total = std::time::Duration::ZERO;
    let mut phase_query = std::time::Duration::ZERO;
    let mut phase_render = std::time::Duration::ZERO;
    let mut phase_convert = std::time::Duration::ZERO;
    let mut phase_push = std::time::Duration::ZERO;
    loop {
        let frame_start = std::time::Instant::now();
        let frame = platform
            .run_frame()
            .map_err(|error| format!("device frame failed: {error:?}"))?;
        frame_work_total += frame_start.elapsed();
        phase_query += frame.phase_timings.map_query;
        phase_render += frame.phase_timings.render;
        phase_convert += frame.phase_timings.convert;
        phase_push += frame.phase_timings.panel_push;
        frames_since_last_log += 1;

        let elapsed = last_log.elapsed();
        if elapsed >= std::time::Duration::from_secs(1) {
            let fps = frames_since_last_log as f32 / elapsed.as_secs_f32();
            let n = u128::from(frames_since_last_log).max(1);
            let avg_work_ms = frame_work_total.as_millis() / n;
            // Log per-phase averages on the same line so we can see at a
            // glance which step is eating the per-frame budget. Phases
            // sum to slightly less than `avg_work_ms` because they
            // exclude input bridging, runtime step, and route-sync
            // bookkeeping — that gap itself is informative.
            log::info!(
                "frame={} fps={:.1} avg_work_ms={} geometry={} | query={}ms render={}ms convert={}ms push={}ms",
                frame.output.frame_index,
                fps,
                avg_work_ms,
                frame.geometry_count,
                phase_query.as_millis() / n,
                phase_render.as_millis() / n,
                phase_convert.as_millis() / n,
                phase_push.as_millis() / n,
            );
            last_log = std::time::Instant::now();
            frames_since_last_log = 0;
            frame_work_total = std::time::Duration::ZERO;
            phase_query = std::time::Duration::ZERO;
            phase_render = std::time::Duration::ZERO;
            phase_convert = std::time::Duration::ZERO;
            phase_push = std::time::Duration::ZERO;
        }
        thread::sleep(board.frame_interval);
    }
}

#[cfg(all(
    target_os = "espidf",
    not(esp32p4),
    not(all(
        not(any(esp32s2, esp32p4)),
        esp_idf_bt_enabled,
        esp_idf_bt_bluedroid_enabled
    ))
))]
pub fn run_device_main() -> Result<(), String> {
    Err("this ESP-IDF target does not expose the standard Bluedroid BLE APIs required by the route-sync GATT server; the ESP32-P4 board has its own entrypoint that drives BLE via the on-board ESP32-C6 over hosted SDIO".to_owned())
}

#[cfg(not(target_os = "espidf"))]
pub fn run_device_main() -> Result<(), String> {
    Err("device entrypoint requires an `espidf` target build".to_owned())
}

fn parse_rmc_sentence(sentence: &str) -> Option<GpsInput> {
    let payload = sentence.trim().strip_prefix('$')?.split('*').next()?;
    let fields: Vec<&str> = payload.split(',').collect();
    if fields.len() < 9 {
        return None;
    }
    if !(fields[0].ends_with("RMC") && fields[2] == "A") {
        return None;
    }
    let lat = parse_nmea_coordinate(fields[3], fields[4], 2)?;
    let lon = parse_nmea_coordinate(fields[5], fields[6], 3)?;
    let speed_mps = fields[7]
        .parse::<f32>()
        .ok()
        .map(|knots| knots * 0.514_444)?;
    let course_rad = if fields[8].is_empty() {
        None
    } else {
        fields[8]
            .parse::<f32>()
            .ok()
            .map(|degrees| degrees.to_radians())
    };

    Some(GpsInput {
        lat_deg: lat,
        lon_deg: lon,
        speed_mps,
        course_rad,
        horizontal_accuracy_m: None,
    })
}

fn parse_nmea_coordinate(raw: &str, hemisphere: &str, degree_digits: usize) -> Option<f64> {
    if raw.len() <= degree_digits {
        return None;
    }
    let (degrees, minutes) = raw.split_at(degree_digits);
    let degrees = degrees.parse::<f64>().ok()?;
    let minutes = minutes.parse::<f64>().ok()?;
    let mut decimal = degrees + (minutes / 60.0);
    if matches!(hemisphere, "S" | "W") {
        decimal = -decimal;
    }
    Some(decimal)
}

#[cfg(test)]
mod tests {
    use std::collections::VecDeque;

    use super::*;
    use crate::board_config::{PanelPixelFormat, TOUCH_I2C_ADDRESS};

    #[derive(Debug, Default)]
    struct MockI2cBus {
        reads: VecDeque<Vec<u8>>,
        writes: Vec<(u16, Vec<u8>)>,
        write_reads: Vec<(u16, Vec<u8>)>,
    }

    impl MockI2cBus {
        fn with_reads(reads: impl IntoIterator<Item = Vec<u8>>) -> Self {
            Self {
                reads: reads.into_iter().collect(),
                writes: Vec::new(),
                write_reads: Vec::new(),
            }
        }
    }

    impl EspIdfI2cBus for MockI2cBus {
        fn write(&mut self, address: u16, payload: &[u8]) -> Result<(), EspIdfError> {
            self.writes.push((address, payload.to_vec()));
            Ok(())
        }

        fn write_read(
            &mut self,
            address: u16,
            tx_payload: &[u8],
            rx_payload: &mut [u8],
        ) -> Result<(), EspIdfError> {
            self.write_reads.push((address, tx_payload.to_vec()));
            let data = self
                .reads
                .pop_front()
                .ok_or_else(|| EspIdfError::Io("missing read fixture".to_owned()))?;
            if data.len() != rx_payload.len() {
                return Err(EspIdfError::Io("unexpected read length".to_owned()));
            }
            rx_payload.copy_from_slice(&data);
            Ok(())
        }
    }

    #[derive(Debug, Default)]
    struct MockPin {
        transitions: Vec<&'static str>,
    }

    impl EspIdfOutputPin for MockPin {
        fn set_low(&mut self) -> Result<(), EspIdfError> {
            self.transitions.push("low");
            Ok(())
        }

        fn set_high(&mut self) -> Result<(), EspIdfError> {
            self.transitions.push("high");
            Ok(())
        }
    }

    #[derive(Debug, Default)]
    struct MockDelay {
        calls: Vec<u32>,
    }

    impl EspIdfDelay for MockDelay {
        fn delay_ms(&mut self, ms: u32) {
            self.calls.push(ms);
        }
    }

    #[derive(Debug, Default)]
    struct MockPanel {
        initialized: Option<DisplayConfig>,
        frames: Vec<(u32, u32, usize)>,
    }

    impl EspIdfPanel for MockPanel {
        fn initialize(&mut self, config: DisplayConfig) -> Result<(), EspIdfError> {
            self.initialized = Some(config);
            Ok(())
        }

        fn present(
            &mut self,
            pixels: &[u8],
            width: u32,
            height: u32,
            _config: DisplayConfig,
        ) -> Result<(), EspIdfError> {
            self.frames.push((width, height, pixels.len()));
            Ok(())
        }
    }

    #[derive(Debug, Default)]
    struct MockGpsSerial {
        sentences: VecDeque<Option<String>>,
    }

    impl MockGpsSerial {
        fn new(sentences: impl IntoIterator<Item = Option<&'static str>>) -> Self {
            Self {
                sentences: sentences
                    .into_iter()
                    .map(|sentence| sentence.map(ToOwned::to_owned))
                    .collect(),
            }
        }
    }

    impl EspIdfGpsSerial for MockGpsSerial {
        fn read_sentence(&mut self) -> Result<Option<String>, EspIdfError> {
            Ok(self.sentences.pop_front().flatten())
        }
    }

    #[test]
    fn gt9271_transport_reads_product_id_and_clears_status_after_report() {
        let bus =
            MockI2cBus::with_reads([b"9271".to_vec(), vec![0x81], vec![1, 0, 0, 0, 0, 0, 0, 0]]);
        let mut transport = EspIdfGt9271Transport::new(
            TOUCH_I2C_ADDRESS,
            bus,
            Some(MockPin::default()),
            Some(MockPin::default()),
            MockDelay::default(),
        );
        let config = TouchControllerConfig::default();

        transport.reset(config).expect("reset");
        assert_eq!(transport.read_product_id().expect("product id"), *b"9271");
        let report = transport.read_touch_report(config).expect("report");

        assert_eq!(report, vec![0x81, 1, 0, 0, 0, 0, 0, 0, 0]);
        assert_eq!(
            transport.bus().write_reads,
            vec![
                (
                    TOUCH_I2C_ADDRESS,
                    GT9271_PRODUCT_ID_REGISTER.to_be_bytes().to_vec()
                ),
                (
                    TOUCH_I2C_ADDRESS,
                    GT9271_STATUS_REGISTER.to_be_bytes().to_vec()
                ),
                (
                    TOUCH_I2C_ADDRESS,
                    GT9271_FIRST_POINT_REGISTER.to_be_bytes().to_vec()
                ),
            ]
        );
        assert_eq!(
            transport.bus().writes,
            vec![(
                TOUCH_I2C_ADDRESS,
                vec![
                    GT9271_STATUS_REGISTER.to_be_bytes()[0],
                    GT9271_STATUS_REGISTER.to_be_bytes()[1],
                    0
                ]
            )]
        );
    }

    #[test]
    fn display_backend_initializes_panel_and_uploads_rgb565_frame() {
        let panel = MockPanel::default();
        let mut backend = EspIdfDisplayBackend::new(panel);
        let config = DisplayConfig {
            viewport_size: runtime_core::api::ViewportSize::new(2, 2),
            pixel_format: PanelPixelFormat::Rgb565Le,
            reset_gpio: None,
            backlight_gpio: None,
        };
        let mut framebuffer = Framebuffer::new(2, 2, PanelPixelFormat::Rgb565Le);
        let mut render = crate::framebuffer::RenderFramebuffer::new(2, 2);
        // Test runs on host, where `RenderFramebuffer = Framebuffer<RgbaPixel>`.
        // 2x2 * 4 bytes/pixel = 16 bytes of RGBA.
        render.pixels_mut().copy_from_slice(&[
            0, 64, 128, 255, 0, 64, 128, 255, 0, 64, 128, 255, 0, 64, 128, 255,
        ]);
        framebuffer.present_from_render(&render);

        backend.initialize(config).expect("panel init");
        backend.present(&framebuffer).expect("panel present");

        assert_eq!(backend.panel().initialized, Some(config));
        assert_eq!(backend.panel().frames, vec![(2, 2, 8)]);
    }

    #[test]
    fn gps_provider_parses_active_rmc_sentence() {
        let mut provider = EspIdfGpsProvider::new(MockGpsSerial::new([Some(
            "$GPRMC,123519,A,6001.2000,N,02456.7000,E,12.5,90.0,230394,003.1,W*6A",
        )]));

        let gps = provider.poll().expect("gps poll").expect("gps fix");

        assert!((gps.lat_deg - 60.02).abs() < 0.0001);
        assert!((gps.lon_deg - 24.945).abs() < 0.0001);
        assert!((gps.speed_mps - 6.43055).abs() < 0.0001);
        assert_eq!(gps.course_rad, Some(90.0_f32.to_radians()));
    }

    #[test]
    fn build_default_device_platform_uses_system_clock_and_default_map_source() {
        let board = BoardConfig::default();
        let bus = MockI2cBus::with_reads([b"9271".to_vec(), vec![0x80]]);
        let transport = EspIdfGt9271Transport::new(
            TOUCH_I2C_ADDRESS,
            bus,
            Some(MockPin::default()),
            Some(MockPin::default()),
            MockDelay::default(),
        );
        let display = EspIdfDisplayBackend::new(MockPanel::default());
        let gps = crate::gps::NullGpsProvider;

        let mut platform =
            build_default_device_platform(board, transport, display, gps).expect("platform");
        // Bypass pairing mode for this end-to-end smoke test — the
        // pairing path has its own dedicated coverage.
        platform.app_mut().force_paired_for_test();

        let frame = platform.run_frame().expect("frame");
        assert_eq!(frame.output.frame_index, 1);
    }
    #[test]
    fn build_headless_route_sync_platform_runs_without_touch_or_gps_hardware() {
        let board = BoardConfig::default();
        let mut platform =
            build_headless_route_sync_platform(board, crate::platform::NullRouteSyncIo)
                .expect("headless route-sync platform");
        platform.app_mut().force_paired_for_test();

        let frame = platform.run_frame().expect("headless frame");
        assert_eq!(frame.output.frame_index, 1);
    }
}
