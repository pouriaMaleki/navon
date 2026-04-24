use std::collections::VecDeque;

use runtime_core::api::GpsSample;

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct GpsInput {
    pub lat_deg: f64,
    pub lon_deg: f64,
    pub speed_mps: f32,
    pub course_rad: Option<f32>,
    pub horizontal_accuracy_m: Option<f32>,
}

impl From<GpsInput> for GpsSample {
    fn from(input: GpsInput) -> Self {
        Self {
            lat_deg: input.lat_deg,
            lon_deg: input.lon_deg,
            speed_mps: input.speed_mps,
            course_rad: input.course_rad,
            horizontal_accuracy_m: input.horizontal_accuracy_m,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GpsError {
    Provider(String),
}

pub trait GpsProvider {
    fn poll(&mut self) -> Result<Option<GpsInput>, GpsError>;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct NullGpsProvider;

impl GpsProvider for NullGpsProvider {
    fn poll(&mut self) -> Result<Option<GpsInput>, GpsError> {
        Ok(None)
    }
}

/// Returns the same fix on every `poll`, forever. Used during device
/// bring-up to park the camera on the embedded map's region while no
/// real GPS hardware is wired, so the runtime actually has geometry to
/// render instead of looking at the Gulf of Guinea at (0, 0).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FixedGpsProvider {
    fix: GpsInput,
}

impl FixedGpsProvider {
    pub const fn new(fix: GpsInput) -> Self {
        Self { fix }
    }
}

impl GpsProvider for FixedGpsProvider {
    fn poll(&mut self) -> Result<Option<GpsInput>, GpsError> {
        Ok(Some(self.fix))
    }
}

#[derive(Debug, Clone, Default)]
pub struct SequenceGpsProvider {
    samples: VecDeque<Option<GpsInput>>,
}

impl SequenceGpsProvider {
    pub fn new(samples: impl IntoIterator<Item = Option<GpsInput>>) -> Self {
        Self {
            samples: samples.into_iter().collect(),
        }
    }
}

impl GpsProvider for SequenceGpsProvider {
    fn poll(&mut self) -> Result<Option<GpsInput>, GpsError> {
        Ok(self.samples.pop_front().flatten())
    }
}
