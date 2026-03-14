use std::time::Duration;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ViewportSize {
    pub width_px: u32,
    pub height_px: u32,
}

impl ViewportSize {
    pub const fn new(width_px: u32, height_px: u32) -> Self {
        Self {
            width_px,
            height_px,
        }
    }

    pub const fn is_empty(self) -> bool {
        self.width_px == 0 || self.height_px == 0
    }
}

impl Default for ViewportSize {
    fn default() -> Self {
        Self::new(0, 0)
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ScreenPoint {
    pub x_px: f32,
    pub y_px: f32,
}

impl ScreenPoint {
    pub const fn new(x_px: f32, y_px: f32) -> Self {
        Self { x_px, y_px }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GpsSample {
    pub lat_deg: f64,
    pub lon_deg: f64,
    pub speed_mps: f32,
    pub course_rad: Option<f32>,
    pub horizontal_accuracy_m: Option<f32>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TouchPhase {
    Started,
    Moved,
    Stationary,
    Ended,
    Cancelled,
}

impl Default for TouchPhase {
    fn default() -> Self {
        Self::Stationary
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TouchContact {
    pub id: u64,
    pub phase: TouchPhase,
    pub position: ScreenPoint,
    pub pressure: Option<f32>,
}

impl TouchContact {
    pub const fn is_active(self) -> bool {
        matches!(
            self.phase,
            TouchPhase::Started | TouchPhase::Moved | TouchPhase::Stationary
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TouchContactFrameError {
    DuplicateContactId(u64),
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct TouchContactFrame {
    pub sequence: u64,
    pub contacts: Vec<TouchContact>,
}

impl TouchContactFrame {
    pub fn new(sequence: u64, contacts: Vec<TouchContact>) -> Result<Self, TouchContactFrameError> {
        validate_contact_ids(&contacts)?;
        Ok(Self { sequence, contacts })
    }

    pub const fn empty(sequence: u64) -> Self {
        Self {
            sequence,
            contacts: Vec::new(),
        }
    }

    pub fn contact_count(&self) -> usize {
        self.contacts.len()
    }

    pub fn active_contact_count(&self) -> usize {
        self.contacts
            .iter()
            .copied()
            .filter(|contact| contact.is_active())
            .count()
    }
}

fn validate_contact_ids(contacts: &[TouchContact]) -> Result<(), TouchContactFrameError> {
    let mut seen = std::collections::BTreeSet::new();
    for contact in contacts {
        if !seen.insert(contact.id) {
            return Err(TouchContactFrameError::DuplicateContactId(contact.id));
        }
    }
    Ok(())
}

#[derive(Debug, Clone, PartialEq)]
pub struct RuntimeInputFrame {
    pub dt: Duration,
    pub gps: Option<GpsSample>,
    pub touch: Option<TouchContactFrame>,
    pub viewport_size: Option<ViewportSize>,
}

impl RuntimeInputFrame {
    pub fn new(dt: Duration) -> Self {
        Self {
            dt,
            gps: None,
            touch: None,
            viewport_size: None,
        }
    }

    pub fn with_gps(mut self, gps: GpsSample) -> Self {
        self.gps = Some(gps);
        self
    }

    pub fn with_touch(mut self, touch: TouchContactFrame) -> Self {
        self.touch = Some(touch);
        self
    }

    pub fn with_viewport(mut self, viewport_size: ViewportSize) -> Self {
        self.viewport_size = Some(viewport_size);
        self
    }
}

impl Default for RuntimeInputFrame {
    fn default() -> Self {
        Self::new(Duration::ZERO)
    }
}
