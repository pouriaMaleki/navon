use crate::api::MapQuerySpec;
use crate::api::{
    CameraMode, CameraOrientationMode, CameraStateSnapshot, DiagnosticsSnapshot, OverlayState,
    RuntimeFrameOutput,
};

pub fn build_frame_output(
    frame_index: u64,
    camera: CameraStateSnapshot,
    map_query: MapQuerySpec,
    diagnostics: Option<DiagnosticsSnapshot>,
    rider_heading_rad: Option<f32>,
) -> RuntimeFrameOutput {
    let overlay = OverlayState {
        north_indicator_visible: true,
        north_up_active: !matches!(camera.orientation_mode, CameraOrientationMode::TravelUpAuto),
        rider_heading_rad: matches!(camera.mode, CameraMode::Riding)
            .then_some(rider_heading_rad)
            .flatten(),
        north_preview_progress: camera.north_preview_progress,
        compass_ack_progress: camera.compass_ack_progress,
    };

    RuntimeFrameOutput {
        frame_index,
        camera,
        map_query,
        overlay,
        diagnostics,
    }
}
