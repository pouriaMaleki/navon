use std::collections::{BTreeMap, VecDeque};

use runtime_core::api::{
    ScreenPoint, TouchContact, TouchContactFrame, TouchContactFrameError, TouchPhase,
};

use crate::board_config::TouchControllerConfig;

const GT9271_REPORT_READY_MASK: u8 = 0x80;
const GT9271_TOUCH_COUNT_MASK: u8 = 0x0f;
const GT9271_CONTACT_RECORD_LEN: usize = 8;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RawTouchContact {
    pub id: u64,
    pub phase: TouchPhase,
    pub position: ScreenPoint,
    pub pressure: Option<f32>,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct TouchInput {
    pub sequence: u64,
    pub contacts: Vec<RawTouchContact>,
}

impl TouchInput {
    pub fn into_runtime_frame(self) -> Result<TouchContactFrame, TouchContactFrameError> {
        let contacts = self
            .contacts
            .into_iter()
            .map(|contact| TouchContact {
                id: contact.id,
                phase: contact.phase,
                position: contact.position,
                pressure: contact.pressure,
            })
            .collect();
        TouchContactFrame::new(self.sequence, contacts)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Gt9271ContactRecord {
    pub track_id: u8,
    pub x: u16,
    pub y: u16,
    pub size: u16,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TouchError {
    Controller(String),
    InvalidReport(String),
    ProductIdMismatch([u8; 4]),
}

pub trait TouchSource {
    fn poll(&mut self) -> Result<Option<TouchInput>, TouchError>;
}

pub trait Gt9271Transport {
    fn reset(&mut self, config: TouchControllerConfig) -> Result<(), TouchError>;
    fn read_product_id(&mut self) -> Result<[u8; 4], TouchError>;
    fn read_touch_report(&mut self, config: TouchControllerConfig) -> Result<Vec<u8>, TouchError>;
}

#[derive(Debug, Clone, Default)]
struct TouchContactState {
    previous_active: BTreeMap<u64, ScreenPoint>,
    next_sequence: u64,
}

impl TouchContactState {
    fn update(
        &mut self,
        config: TouchControllerConfig,
        active_contacts: Vec<Gt9271ContactRecord>,
    ) -> Option<TouchInput> {
        let mut contacts = Vec::new();
        let mut current_active = BTreeMap::new();

        for contact in active_contacts {
            let id = u64::from(contact.track_id);
            let position = normalize_screen_point(contact.x, contact.y, config);
            let phase = match self.previous_active.get(&id) {
                None => TouchPhase::Started,
                Some(previous) if *previous == position => TouchPhase::Stationary,
                Some(_) => TouchPhase::Moved,
            };
            current_active.insert(id, position);
            contacts.push(RawTouchContact {
                id,
                phase,
                position,
                pressure: Some(normalize_pressure(contact.size)),
            });
        }

        for (&id, &position) in &self.previous_active {
            if !current_active.contains_key(&id) {
                contacts.push(RawTouchContact {
                    id,
                    phase: TouchPhase::Ended,
                    position,
                    pressure: None,
                });
            }
        }

        self.previous_active = current_active;
        if contacts.is_empty() {
            return None;
        }

        contacts.sort_by_key(|contact| contact.id);
        self.next_sequence += 1;
        Some(TouchInput {
            sequence: self.next_sequence,
            contacts,
        })
    }
}

#[derive(Debug, Clone)]
pub struct PollingTouchSource<T>
where
    T: Gt9271Transport,
{
    config: TouchControllerConfig,
    transport: T,
    state: TouchContactState,
    initialized: bool,
}

impl<T> PollingTouchSource<T>
where
    T: Gt9271Transport,
{
    pub fn new(config: TouchControllerConfig, transport: T) -> Self {
        Self {
            config,
            transport,
            state: TouchContactState::default(),
            initialized: false,
        }
    }

    fn initialize_if_needed(&mut self) -> Result<(), TouchError> {
        if self.initialized {
            return Ok(());
        }

        self.transport.reset(self.config)?;
        let product_id = self.transport.read_product_id()?;
        if &product_id != b"9271" {
            return Err(TouchError::ProductIdMismatch(product_id));
        }
        self.initialized = true;
        Ok(())
    }
}

impl<T> TouchSource for PollingTouchSource<T>
where
    T: Gt9271Transport,
{
    fn poll(&mut self) -> Result<Option<TouchInput>, TouchError> {
        self.initialize_if_needed()?;
        let report = self.transport.read_touch_report(self.config)?;
        let contacts = decode_touch_report(&report, self.config)?;
        Ok(self.state.update(self.config, contacts))
    }
}

#[derive(Debug, Clone, Default)]
pub struct MockGt9271Transport {
    product_id: [u8; 4],
    reports: VecDeque<Vec<u8>>,
}

impl MockGt9271Transport {
    pub fn new(reports: impl IntoIterator<Item = Vec<u8>>) -> Self {
        Self {
            product_id: *b"9271",
            reports: reports.into_iter().collect(),
        }
    }

    pub fn with_product_id(mut self, product_id: [u8; 4]) -> Self {
        self.product_id = product_id;
        self
    }
}

impl Gt9271Transport for MockGt9271Transport {
    fn reset(&mut self, _config: TouchControllerConfig) -> Result<(), TouchError> {
        Ok(())
    }

    fn read_product_id(&mut self) -> Result<[u8; 4], TouchError> {
        Ok(self.product_id)
    }

    fn read_touch_report(&mut self, _config: TouchControllerConfig) -> Result<Vec<u8>, TouchError> {
        Ok(self
            .reports
            .pop_front()
            .unwrap_or_else(|| vec![GT9271_REPORT_READY_MASK]))
    }
}

pub fn decode_touch_report(
    report: &[u8],
    config: TouchControllerConfig,
) -> Result<Vec<Gt9271ContactRecord>, TouchError> {
    let Some(&status) = report.first() else {
        return Err(TouchError::InvalidReport(
            "gt9271 report is missing the status byte".to_owned(),
        ));
    };
    if status & GT9271_REPORT_READY_MASK == 0 {
        return Ok(Vec::new());
    }

    let touch_count = usize::from(status & GT9271_TOUCH_COUNT_MASK);
    if touch_count > usize::from(config.max_contacts) {
        return Err(TouchError::InvalidReport(format!(
            "touch count {touch_count} exceeds configured maximum {}",
            config.max_contacts
        )));
    }

    let expected_len = 1 + (touch_count * GT9271_CONTACT_RECORD_LEN);
    if report.len() < expected_len {
        return Err(TouchError::InvalidReport(format!(
            "gt9271 report length {} is too short for {touch_count} contacts",
            report.len()
        )));
    }

    let mut contacts = Vec::new();
    for index in 0..touch_count {
        let base = 1 + (index * GT9271_CONTACT_RECORD_LEN);
        let track_id = report[base];
        let x = u16::from_le_bytes([report[base + 1], report[base + 2]]);
        let y = u16::from_le_bytes([report[base + 3], report[base + 4]]);
        let size = u16::from_le_bytes([report[base + 5], report[base + 6]]);

        if x > config.controller_max_x || y > config.controller_max_y {
            continue;
        }

        contacts.push(Gt9271ContactRecord {
            track_id,
            x,
            y,
            size,
        });
    }
    contacts.sort_by_key(|contact| contact.track_id);
    Ok(contacts)
}

fn normalize_screen_point(x: u16, y: u16, config: TouchControllerConfig) -> ScreenPoint {
    ScreenPoint::new(
        normalize_axis(x, config.controller_max_x, config.logical_width_px),
        normalize_axis(y, config.controller_max_y, config.logical_height_px),
    )
}

fn normalize_axis(raw: u16, controller_max: u16, logical_size: u32) -> f32 {
    if controller_max == 0 || logical_size <= 1 {
        return 0.0;
    }
    let logical_max = logical_size.saturating_sub(1) as f32;
    (f32::from(raw.min(controller_max)) / f32::from(controller_max)) * logical_max
}

fn normalize_pressure(size: u16) -> f32 {
    (f32::from(size.min(1024)) / 1024.0).clamp(0.0, 1.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::board_config::TouchControllerConfig;

    fn config() -> TouchControllerConfig {
        TouchControllerConfig::default()
    }

    fn report(contacts: &[(u8, u16, u16, u16)]) -> Vec<u8> {
        let mut report = vec![GT9271_REPORT_READY_MASK | contacts.len() as u8];
        for (id, x, y, size) in contacts {
            report.push(*id);
            report.extend_from_slice(&x.to_le_bytes());
            report.extend_from_slice(&y.to_le_bytes());
            report.extend_from_slice(&size.to_le_bytes());
            report.push(0);
        }
        report
    }

    #[test]
    fn converts_raw_contacts_into_runtime_contacts() {
        let touch = TouchInput {
            sequence: 5,
            contacts: vec![RawTouchContact {
                id: 9,
                phase: TouchPhase::Started,
                position: ScreenPoint::new(12.0, 34.0),
                pressure: Some(0.7),
            }],
        };

        let frame = touch.into_runtime_frame().expect("valid touch input");
        assert_eq!(frame.sequence, 5);
        assert_eq!(frame.contacts.len(), 1);
        assert_eq!(frame.contacts[0].id, 9);
    }

    #[test]
    fn decodes_gt9271_report_and_filters_out_of_bounds_contacts() {
        let decoded =
            decode_touch_report(&report(&[(2, 300, 400, 512), (3, 950, 10, 256)]), config())
                .expect("valid report");

        assert_eq!(
            decoded,
            vec![Gt9271ContactRecord {
                track_id: 2,
                x: 300,
                y: 400,
                size: 512,
            }]
        );
    }

    #[test]
    fn polling_touch_source_generates_started_stationary_and_ended_contacts() {
        let reports = vec![
            report(&[(1, 100, 200, 512)]),
            report(&[(1, 100, 200, 512)]),
            vec![GT9271_REPORT_READY_MASK],
        ];
        let mut source = PollingTouchSource::new(config(), MockGt9271Transport::new(reports));

        let first = source.poll().expect("first report").expect("touch frame");
        let second = source.poll().expect("second report").expect("touch frame");
        let ended = source.poll().expect("ended report").expect("touch frame");

        assert_eq!(first.contacts[0].phase, TouchPhase::Started);
        assert_eq!(second.contacts[0].phase, TouchPhase::Stationary);
        assert_eq!(ended.contacts[0].phase, TouchPhase::Ended);
    }

    #[test]
    fn polling_touch_source_rejects_unexpected_product_id() {
        let mut source = PollingTouchSource::new(
            config(),
            MockGt9271Transport::new([]).with_product_id(*b"1111"),
        );

        assert_eq!(source.poll(), Err(TouchError::ProductIdMismatch(*b"1111")));
    }

    #[test]
    fn normalizes_contacts_into_logical_viewport_space() {
        let config = TouchControllerConfig {
            controller_max_x: 1_023,
            controller_max_y: 511,
            logical_width_px: 800,
            logical_height_px: 480,
            ..config()
        };

        let point = normalize_screen_point(1_023, 511, config);

        assert!((point.x_px - 799.0).abs() < 0.01);
        assert!((point.y_px - 479.0).abs() < 0.01);
    }
}
