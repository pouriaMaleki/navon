use super::config::{NormalizedScreenPoint, SpeedUnit};
use super::diagnostics::DiagnosticsSnapshot;
use super::query::{MapQuerySpec, WorldPoint};
use crate::route::RouteRenderState;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CameraMode {
    Riding,
    Stopped,
}

impl Default for CameraMode {
    fn default() -> Self {
        Self::Stopped
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CameraOrientationMode {
    StoppedNorthUp,
    HeadingAcquisition,
    TravelUpAuto,
    NorthPreview,
    NorthLocked,
}

impl Default for CameraOrientationMode {
    fn default() -> Self {
        Self::StoppedNorthUp
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct CameraStateSnapshot {
    pub mode: CameraMode,
    pub orientation_mode: CameraOrientationMode,
    pub focus_world: WorldPoint,
    pub center_world: WorldPoint,
    pub zoom: f32,
    pub orientation_rad: f32,
    pub north_preview_progress: Option<f32>,
    pub compass_ack_progress: f32,
    pub rider_anchor: NormalizedScreenPoint,
    pub follow_locked: bool,
    pub recenter_active: bool,
}

impl Default for CameraStateSnapshot {
    fn default() -> Self {
        Self {
            mode: CameraMode::Stopped,
            orientation_mode: CameraOrientationMode::StoppedNorthUp,
            focus_world: WorldPoint::ORIGIN,
            center_world: WorldPoint::ORIGIN,
            zoom: 0.0,
            orientation_rad: 0.0,
            north_preview_progress: None,
            compass_ack_progress: 0.0,
            rider_anchor: NormalizedScreenPoint::CENTER,
            follow_locked: false,
            recenter_active: false,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct OverlayState {
    pub north_indicator_visible: bool,
    pub north_up_active: bool,
    pub rider_heading_rad: Option<f32>,
    pub north_preview_progress: Option<f32>,
    pub compass_ack_progress: f32,
    pub speed_panel_visible: bool,
    pub speed_display_value: u16,
    pub speed_unit: SpeedUnit,
    /// True when the map source has nothing to render yet (tiles loading,
    /// first query in flight). Drives the "loading" affordance in spec line 18.
    pub map_tiles_loading: bool,
}

impl Default for OverlayState {
    fn default() -> Self {
        Self {
            north_indicator_visible: true,
            north_up_active: true,
            rider_heading_rad: None,
            north_preview_progress: None,
            compass_ack_progress: 0.0,
            speed_panel_visible: false,
            speed_display_value: 0,
            speed_unit: SpeedUnit::Kph,
            map_tiles_loading: false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct RuntimeFrameOutput {
    pub frame_index: u64,
    pub camera: CameraStateSnapshot,
    pub map_query: MapQuerySpec,
    pub overlay: OverlayState,
    pub route: RouteRenderState,
    pub diagnostics: Option<DiagnosticsSnapshot>,
}
