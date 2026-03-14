pub mod adapter;
pub mod bindings;
pub mod input_bridge;
pub mod output_bridge;
pub mod panic_hook;

#[derive(Debug, Default)]
pub struct WasmAdapter;
