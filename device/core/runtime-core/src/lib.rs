#![cfg_attr(not(feature = "std"), no_std)]

extern crate alloc;

pub mod api;
pub mod camera;
pub mod diagnostics;
pub mod input;
pub mod map;
pub mod motion;
pub mod output;
pub mod overlay_ui;
pub mod route;
pub mod schedule;

use api::{RuntimeConfig, RuntimeFrameOutput, RuntimeInputFrame};
use schedule::RuntimeRunner;

pub struct RuntimeCore {
    runner: RuntimeRunner,
}

impl RuntimeCore {
    pub fn new(config: RuntimeConfig) -> Self {
        Self {
            runner: RuntimeRunner::new(config),
        }
    }

    pub fn config(&self) -> &RuntimeConfig {
        self.runner.config()
    }

    pub fn frame_index(&self) -> u64 {
        self.runner.frame_index()
    }

    pub fn step(&mut self, input: RuntimeInputFrame) -> RuntimeFrameOutput {
        self.runner.step(input)
    }
}

impl Default for RuntimeCore {
    fn default() -> Self {
        Self::new(RuntimeConfig::default())
    }
}
