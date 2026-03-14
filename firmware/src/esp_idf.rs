use runtime_core::api::RuntimeConfig;
use runtime_core::map::MapSource;

use crate::app::App;
use crate::board_config::{BoardConfig, DisplayConfig, TouchControllerConfig};
use crate::display::{DisplayBackend, DisplayError};
use crate::framebuffer::Framebuffer;
use crate::gps::{GpsError, GpsInput, GpsProvider};
use crate::map_source::MapSourceBridge;
use crate::platform::{RuntimePlatform, SystemFrameClock};
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

pub type DevicePlatform<T, R, I, D, G, S, P> = RuntimePlatform<
    PollingTouchSource<EspIdfGt9271Transport<T, R, I, D>>,
    G,
    SystemFrameClock,
    S,
    EspIdfDisplayBackend<P>,
>;

pub type DefaultDevicePlatform<T, R, I, D, G, P> =
    DevicePlatform<T, R, I, D, G, MapSourceBridge, P>;

pub type DevicePlatformResult<T, R, I, D, G, S, P> =
    Result<DevicePlatform<T, R, I, D, G, S, P>, DisplayError>;

pub type DefaultDevicePlatformResult<T, R, I, D, G, P> =
    Result<DefaultDevicePlatform<T, R, I, D, G, P>, DisplayError>;

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
    let runtime_config = RuntimeConfig {
        viewport_size: board.viewport_size,
        ..RuntimeConfig::default()
    };
    let touch_source = PollingTouchSource::new(board.touch, touch_transport);
    let app = App::with_parts(board, runtime_config, map_source, display_backend)?;
    Ok(RuntimePlatform::new(
        app,
        touch_source,
        gps_provider,
        SystemFrameClock::new(board.frame_interval),
    ))
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

#[cfg(target_os = "espidf")]
pub fn run_device_main() -> Result<(), String> {
    Err("real ESP-IDF peripheral acquisition is not linked in this workspace build".to_owned())
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
        render.pixels_mut().copy_from_slice(&[0, 64, 128, 255]);
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
}
