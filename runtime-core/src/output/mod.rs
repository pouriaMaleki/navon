use crate::api::MapQuerySpec;
use crate::api::{
    CameraMode, CameraStateSnapshot, DiagnosticsSnapshot, OverlayState, RuntimeFrameOutput,
};

pub fn build_frame_output(
    frame_index: u64,
    camera: CameraStateSnapshot,
    map_query: MapQuerySpec,
    diagnostics: Option<DiagnosticsSnapshot>,
    north_up_active: bool,
    rider_heading_rad: Option<f32>,
) -> RuntimeFrameOutput {
    let overlay = OverlayState {
        north_indicator_visible: true,
        north_up_active,
        rider_heading_rad: matches!(camera.mode, CameraMode::Riding)
            .then_some(rider_heading_rad)
            .flatten(),
    };

    RuntimeFrameOutput {
        frame_index,
        camera,
        map_query,
        overlay,
        diagnostics,
    }
}
