use crate::api::{OverlayState, RuntimeConfig, ScreenPoint, SpeedUnit, TapEvent, ViewportSize};
use crate::motion::MotionState;

#[derive(Debug, Clone, PartialEq)]
pub struct OverlayUiState {
    speed_unit: SpeedUnit,
}

impl OverlayUiState {
    pub fn new(default_speed_unit: SpeedUnit) -> Self {
        Self {
            speed_unit: default_speed_unit,
        }
    }

    pub fn speed_unit(&self) -> SpeedUnit {
        self.speed_unit
    }

    pub fn advance(
        &mut self,
        motion: &MotionState,
        tap: Option<&TapEvent>,
        viewport_size: ViewportSize,
        config: &RuntimeConfig,
    ) {
        let Some(tap) = tap else {
            return;
        };
        if !speed_panel_visible(motion) {
            return;
        }
        if speed_panel_hit(tap.position, viewport_size, config) {
            self.speed_unit = self.speed_unit.toggled();
        }
    }

    pub fn build_overlay_state(&self, motion: &MotionState, overlay: &mut OverlayState) {
        overlay.speed_panel_visible = speed_panel_visible(motion);
        overlay.speed_display_value = self
            .speed_unit
            .rounded_display_value_from_mps(motion.speed_mps);
        overlay.speed_unit = self.speed_unit;
    }
}

impl Default for OverlayUiState {
    fn default() -> Self {
        Self::new(SpeedUnit::default())
    }
}

fn speed_panel_visible(motion: &MotionState) -> bool {
    motion.is_moving
}

fn speed_panel_hit(
    tap_position: ScreenPoint,
    viewport_size: ViewportSize,
    _config: &RuntimeConfig,
) -> bool {
    if viewport_size.is_empty() {
        return false;
    }
    tap_position.y_px >= viewport_size.height_px as f32 * 0.75
}
