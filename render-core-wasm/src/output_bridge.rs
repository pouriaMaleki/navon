use runtime_core::api::{CameraMode, RuntimeFrameOutput};

#[derive(Debug, Clone, PartialEq)]
pub struct JsFrameOutput {
    pub frame_index: u64,
    pub camera_mode: &'static str,
    pub zoom: f32,
    pub orientation_rad: f32,
}

impl From<&RuntimeFrameOutput> for JsFrameOutput {
    fn from(output: &RuntimeFrameOutput) -> Self {
        Self {
            frame_index: output.frame_index,
            camera_mode: match output.camera.mode {
                CameraMode::Riding => "riding",
                CameraMode::Stopped => "stopped",
            },
            zoom: output.camera.zoom,
            orientation_rad: output.camera.orientation_rad,
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct OutputBridge;

impl OutputBridge {
    pub fn present(&self, output: &RuntimeFrameOutput) -> JsFrameOutput {
        JsFrameOutput::from(output)
    }
}
