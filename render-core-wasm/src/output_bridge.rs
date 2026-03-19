use serde::Serialize;

use runtime_core::api::{CameraMode, CameraOrientationMode, MapQueryResult, RuntimeFrameOutput};

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct JsFrameState {
    #[serde(rename = "frameIndex")]
    pub frame_index: u64,
    #[serde(rename = "cameraMode")]
    pub camera_mode: &'static str,
    #[serde(rename = "cameraOrientationMode")]
    pub camera_orientation_mode: &'static str,
    pub zoom: f32,
    #[serde(rename = "orientationRad")]
    pub orientation_rad: f32,
    #[serde(rename = "followLocked")]
    pub follow_locked: bool,
    #[serde(rename = "recenterActive")]
    pub recenter_active: bool,
    #[serde(rename = "northUpActive")]
    pub north_up_active: bool,
    #[serde(rename = "speedVisible")]
    pub speed_visible: bool,
    #[serde(rename = "speedValue")]
    pub speed_value: u16,
    #[serde(rename = "speedUnit")]
    pub speed_unit: &'static str,
    #[serde(rename = "geometryCount")]
    pub geometry_count: usize,
}

impl JsFrameState {
    pub fn from_output(output: &RuntimeFrameOutput, geometry: &MapQueryResult) -> Self {
        Self {
            frame_index: output.frame_index,
            camera_mode: match output.camera.mode {
                CameraMode::Riding => "riding",
                CameraMode::Stopped => "stopped",
            },
            camera_orientation_mode: match output.camera.orientation_mode {
                CameraOrientationMode::StoppedNorthUp => "stopped_north_up",
                CameraOrientationMode::HeadingAcquisition => "heading_acquisition",
                CameraOrientationMode::TravelUpAuto => "travel_up_auto",
                CameraOrientationMode::NorthPreview => "north_preview",
                CameraOrientationMode::NorthLocked => "north_locked",
            },
            zoom: output.camera.zoom,
            orientation_rad: output.camera.orientation_rad,
            follow_locked: output.camera.follow_locked,
            recenter_active: output.camera.recenter_active,
            north_up_active: output.overlay.north_up_active,
            speed_visible: output.overlay.speed_panel_visible,
            speed_value: output.overlay.speed_display_value,
            speed_unit: output.overlay.speed_unit.as_str(),
            geometry_count: geometry.geometry.len(),
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct OutputBridge;

impl OutputBridge {
    pub fn present(&self, output: &JsFrameState) -> Result<String, String> {
        serde_json::to_string(output)
            .map_err(|error| format!("failed to serialize frame output: {error}"))
    }
}
