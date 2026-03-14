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
