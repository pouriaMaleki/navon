use wasm_bindgen::prelude::*;

use runtime_core::api::RuntimeConfig;

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
    pub fn new(_profile: u32) -> Self {
        panic_hook::install();
        Self {
            state: AdapterState::new(RuntimeConfig::default()),
            input_bridge: InputBridge::default(),
            output_bridge: OutputBridge::default(),
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
