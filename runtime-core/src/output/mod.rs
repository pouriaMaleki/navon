use crate::api::MapQuerySpec;
use crate::api::{
    CameraMode, CameraOrientationMode, CameraStateSnapshot, DiagnosticsSnapshot, OverlayState,
    RuntimeFrameOutput,
};
use crate::motion::MotionState;
use crate::overlay_ui::OverlayUiState;

pub fn build_frame_output(
    frame_index: u64,
    camera: CameraStateSnapshot,
    map_query: MapQuerySpec,
    diagnostics: Option<DiagnosticsSnapshot>,
    rider_heading_rad: Option<f32>,
    motion: &MotionState,
    overlay_ui: &OverlayUiState,
) -> RuntimeFrameOutput {
    let mut overlay = OverlayState {
        north_indicator_visible: true,
        north_up_active: !matches!(camera.orientation_mode, CameraOrientationMode::TravelUpAuto),
        rider_heading_rad: matches!(camera.mode, CameraMode::Riding)
            .then_some(rider_heading_rad)
            .flatten(),
        north_preview_progress: camera.north_preview_progress,
        compass_ack_progress: camera.compass_ack_progress,
        ..OverlayState::default()
    };
    overlay_ui.build_overlay_state(motion, &mut overlay);

    RuntimeFrameOutput {
        frame_index,
        camera,
        map_query,
        overlay,
        diagnostics,
    }
}
