use runtime_core::api::RuntimeConfig;
use runtime_core::map::MapSource;

use crate::app::{App, AppBuildError};
use crate::board_config::{BoardConfig, DisplayConfig, TouchControllerConfig};
use crate::display::{DisplayBackend, DisplayError, MemoryDisplayBackend};
use crate::framebuffer::Framebuffer;
use crate::gps::{GpsError, GpsInput, GpsProvider, NullGpsProvider};
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

#[derive(Debug, Clone)]
pub struct EspIdfGpsProvider<S>
where
    S: EspIdfGpsSerial,
{
    serial: S,
}

impl<S> EspIdfGpsProvider<S>
where
    S: EspIdfGpsSerial,
{
    pub fn new(serial: S) -> Self {
        Self { serial }
    }
}

impl<S> GpsProvider for EspIdfGpsProvider<S>
where
    S: EspIdfGpsSerial,
{
    fn poll(&mut self) -> Result<Option<GpsInput>, GpsError> {
        match self
            .serial
            .read_sentence()
            .map_err(|error| GpsError::Provider(format!("{error:?}")))?
        {
            Some(sentence) => Ok(parse_rmc_sentence(&sentence)),
            None => Ok(None),
        }
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

// ESP32-P4 bring-up entrypoint. The P4 has no on-chip radio, so Bluedroid is
// not available through `esp-idf-svc`; a BLE route-sync service must come from
// the external ESP32-C6 radio path. For the first device bring-up we boot the
// same headless runtime with a `NullRouteSyncIo`, which proves the toolchain
// end-to-end (flash, boot, run frames, log over USB-serial) without display,
// touch, GPS, or BLE wiring.
#[cfg(all(target_os = "espidf", esp32p4))]
pub fn run_device_main() -> Result<(), String> {
    use std::thread;

    use esp_idf_svc::log::EspLogger;
    use esp_idf_svc::sys;
    use map_runtime::EmbeddedMapSource;
    use runtime_core::api::RuntimeConfig;

    use crate::app::App;
    use crate::gps::{FixedGpsProvider, GpsInput};
    use crate::map_source::MapSourceBridge;
    use crate::mipi_dsi::{
        self, MipiDsiPanel, PanelGpios, waveshare_3p4c_st77922_config,
    };
    use crate::platform::{NullRouteSyncIo, NullTouchSource, RuntimePlatform, SystemFrameClock};
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

    let board = BoardConfig::default();

    // Load the map first so we can log its bounds and derive an initial
    // camera position from its centroid. Without a real GPS fix the camera
    // would otherwise sit at world (0, 0), which is outside the embedded
    // map's coverage, and `MapSource::query` would return zero geometry —
    // the exact symptom we saw on the first flash.
    let map = EmbeddedMapSource::default();
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
    // Diagnostic: scan the I²C bus before touching the panel. Waveshare
    // ESP32-P4 boards often route LCD-reset/backlight/touch-reset through
    // a CH422G I/O expander (0x20-0x27 typical) or similar, not through
    // direct ESP32 GPIOs. This one-shot probe logs every ACKing address
    // so we can see whether the 3.4C has an expander and at what address.
    if let Err(error) =
        crate::i2c_scan::scan_bus(crate::i2c_scan::DEFAULT_SDA_GPIO, crate::i2c_scan::DEFAULT_SCL_GPIO)
    {
        log::warn!("i2c scan failed: {:?}", error);
    }

    // Bring up the Waveshare 3.4" round 800x800 MIPI-DSI panel.
    //
    // Order matters:
    //   1. Acquire the LDO3 rail for the DSI PHY — the peripheral will
    //      not lock without it.
    //   2. Enable the panel's backlight GPIO so anything we eventually
    //      push to it is actually visible.
    //   3. Pulse the panel reset line so the ST77922 starts from a clean
    //      state before we send DCS init commands.
    //   4. Construct `MipiDsiPanel` (sets up DSI bus + DBI command IO).
    //   5. Wrap it in `EspIdfDisplayBackend` so the `App` drives it
    //      through the existing `DisplayBackend` trait.
    //   6. The `backend.initialize` call (inside `App::with_parts_*`)
    //      performs the DPI panel creation, runs the init sequence, and
    //      leaves the panel ready to accept `draw_bitmap` calls.
    let gpios = PanelGpios::WAVESHARE_3P4C;
    // Keep the LDO handle alive for the life of the platform; dropping
    // it powers the PHY rail off.
    let _phy_ldo = mipi_dsi::acquire_mipi_dsi_phy_power()
        .map_err(|error| format!("failed to power MIPI-DSI PHY: {error:?}"))?;
    // LDO 4 commonly powers the panel's AVDD rail on Waveshare ESP32-P4
    // carriers. If it's already on (e.g. tied to a different rail on
    // this revision) the acquire is a no-op; we just keep the handle.
    let _avdd_ldo = match mipi_dsi::acquire_panel_avdd_power(3300) {
        Ok(handle) => {
            log::info!("ldo: panel AVDD enabled on LDO4 @ 3300 mV");
            Some(handle)
        }
        Err(error) => {
            log::warn!("ldo: panel AVDD acquire failed (continuing anyway): {error:?}");
            None
        }
    };

    // Diagnostic: cycle a set of backlight-candidate GPIOs through HIGH
    // → LOW → HIGH so the operator can see which one (if any) drives
    // the actual backlight. Each pin dwells in each state for 700 ms
    // — enough for visual confirmation but short enough that boot still
    // reaches the render loop in a reasonable time. Pins that aren't
    // wired to anything cost nothing; pins that are wired settle to HIGH
    // at the end of the test, so the right one keeps backlight on.
    log::info!("blink test: probing backlight-candidate GPIOs (watch the panel)");
    if let Err(error) =
        mipi_dsi::blink_test_candidates(&[22, 23, 26, 27, 28, 29], 700)
    {
        log::warn!("blink test failed: {error:?}");
    }

    if let Some(bl) = gpios.backlight {
        mipi_dsi::enable_backlight(bl)
            .map_err(|error| format!("failed to enable panel backlight: {error:?}"))?;
    }
    if let Some(rst) = gpios.reset {
        mipi_dsi::pulse_reset(rst)
            .map_err(|error| format!("failed to reset panel: {error:?}"))?;
    }
    let panel = MipiDsiPanel::new(waveshare_3p4c_st77922_config())
        .map_err(|error| format!("failed to create MIPI-DSI bus: {error:?}"))?;
    let display_backend = EspIdfDisplayBackend::new(panel);
    log::info!("mipi-dsi panel: Waveshare 3.4C ST77922, 800x800, 2 lanes @ 1000 Mbps");

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

    let (seed_lat, seed_lon) = center_lat_lon.unwrap_or((60.1699, 24.9384)); // Helsinki fallback
    let gps = FixedGpsProvider::new(GpsInput {
        lat_deg: seed_lat,
        lon_deg: seed_lon,
        speed_mps: 0.0,
        course_rad: None,
        horizontal_accuracy_m: Some(5.0),
    });
    log::info!("seeded fixed GPS fix at lat={:.4} lon={:.4}", seed_lat, seed_lon);

    let mut platform = RuntimePlatform::with_route_sync(
        app,
        NullTouchSource,
        gps,
        SystemFrameClock::new(board.frame_interval),
        NullRouteSyncIo,
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
    loop {
        let frame_start = std::time::Instant::now();
        let frame = platform
            .run_frame()
            .map_err(|error| format!("device frame failed: {error:?}"))?;
        frame_work_total += frame_start.elapsed();
        frames_since_last_log += 1;

        let elapsed = last_log.elapsed();
        if elapsed >= std::time::Duration::from_secs(1) {
            let fps = frames_since_last_log as f32 / elapsed.as_secs_f32();
            let avg_work_ms = if frames_since_last_log > 0 {
                frame_work_total.as_millis() / u128::from(frames_since_last_log)
            } else {
                0
            };
            log::info!(
                "frame={} fps={:.1} avg_work_ms={} geometry={}",
                frame.output.frame_index,
                fps,
                avg_work_ms,
                frame.geometry_count
            );
            last_log = std::time::Instant::now();
            frames_since_last_log = 0;
            frame_work_total = std::time::Duration::ZERO;
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
    Err("this ESP-IDF target does not expose the standard Bluedroid BLE APIs required by the route-sync GATT server; on the Waveshare ESP32-P4 board this must be implemented through the external radio path instead of esp-idf-svc::bt".to_owned())
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
        let mut render = render_core::raster::Framebuffer::new(2, 2);
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

        let frame = platform.run_frame().expect("frame");
        assert_eq!(frame.output.frame_index, 1);
    }
    #[test]
    fn build_headless_route_sync_platform_runs_without_touch_or_gps_hardware() {
        let board = BoardConfig::default();
        let mut platform =
            build_headless_route_sync_platform(board, crate::platform::NullRouteSyncIo)
                .expect("headless route-sync platform");

        let frame = platform.run_frame().expect("headless frame");
        assert_eq!(frame.output.frame_index, 1);
    }
}
