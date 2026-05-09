use crate::api::{
    CameraStateSnapshot, DiagnosticsSnapshot, MapQuerySpec, RuntimeInputFrame, ViewportSize,
};

pub fn build_snapshot(
    frame_index: u64,
    viewport_size: ViewportSize,
    input: &RuntimeInputFrame,
    camera: &CameraStateSnapshot,
    map_query: &MapQuerySpec,
) -> DiagnosticsSnapshot {
    let (touch_contact_count, active_touch_contacts) = input
        .touch
        .as_ref()
        .map(|touch| (touch.contact_count(), touch.active_contact_count()))
        .unwrap_or((0, 0));

    DiagnosticsSnapshot {
        frame_index,
        viewport_size,
        has_gps_fix: input.gps.is_some(),
        touch_contact_count,
        active_touch_contacts,
        camera_mode: camera.mode,
        presentation_band: map_query.presentation_band,
        meters_per_pixel: map_query.meters_per_pixel,
    }
}
