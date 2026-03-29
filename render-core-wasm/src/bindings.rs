use wasm_bindgen::prelude::*;

use runtime_core::api::{RouteAlertVerbosity, RuntimeConfig, SpeedUnit};

use crate::adapter::AdapterState;
use crate::input_bridge::InputBridge;
use crate::output_bridge::OutputBridge;
use crate::panic_hook;

#[wasm_bindgen]
pub struct MinimapWasmEmulator {
    state: AdapterState,
    input_bridge: InputBridge,
    output_bridge: OutputBridge,
}

#[wasm_bindgen]
impl MinimapWasmEmulator {
    #[wasm_bindgen(constructor)]
    pub fn new(
        _profile: u32,
        default_speed_unit: Option<String>,
        route_alert_verbosity: Option<String>,
    ) -> Self {
        panic_hook::install();
        let mut config = RuntimeConfig::default();
        if let Some(unit) = default_speed_unit
            .as_deref()
            .and_then(SpeedUnit::from_storage_str)
        {
            config.default_speed_unit = unit;
        }
        if let Some(verbosity) = route_alert_verbosity
            .as_deref()
            .and_then(RouteAlertVerbosity::from_storage_str)
        {
            config.route_alert_verbosity = verbosity;
        }
        Self {
            state: AdapterState::new(config),
            input_bridge: InputBridge,
            output_bridge: OutputBridge,
        }
    }

    pub fn reset(&mut self) {
        self.state.reset();
    }

    pub fn step_frame(&mut self, dt_ms: f64, frame_json: &str) -> Result<String, JsValue> {
        let input = self
            .input_bridge
            .frame_from_json(dt_ms, frame_json)
            .map_err(|error| JsValue::from_str(&error))?;
        let snapshot = self.state.step(input);
        self.output_bridge
            .present(&snapshot)
            .map_err(|error| JsValue::from_str(&error))
    }

    pub fn pixels_ptr(&self) -> *const u8 {
        self.state.pixels().as_ptr()
    }

    pub fn pixels_len(&self) -> usize {
        self.state.pixels().len()
    }

    pub fn width(&self) -> u32 {
        self.state.width()
    }

    pub fn height(&self) -> u32 {
        self.state.height()
    }
}
