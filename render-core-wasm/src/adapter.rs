use runtime_core::RuntimeCore;
use runtime_core::api::{RuntimeConfig, RuntimeFrameOutput, RuntimeInputFrame};

pub struct AdapterState {
    runtime: RuntimeCore,
}

impl AdapterState {
    pub fn new(config: RuntimeConfig) -> Self {
        Self {
            runtime: RuntimeCore::new(config),
        }
    }

    pub fn step(&mut self, input: RuntimeInputFrame) -> RuntimeFrameOutput {
        self.runtime.step(input)
    }
}

impl Default for AdapterState {
    fn default() -> Self {
        Self::new(RuntimeConfig::default())
    }
}
