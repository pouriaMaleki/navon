import type { EmulatorState, RenderProgram } from "../core/types";
import initWasm, {
  MinimapWasmEmulator,
  type InitOutput
} from "../../wasm-pkg/render_core_wasm.js";

import type { FrameBuffer } from "../core/framebuffer";

export type WasmProgramState = {
  emu: MinimapWasmEmulator;
};

export async function createWasmMinimapProgram(profileId = 0): Promise<{
  initialState: WasmProgramState;
  program: RenderProgram<WasmProgramState>;
}> {
  const wasm: InitOutput = await initWasm();
  const emu = new MinimapWasmEmulator(profileId);

  const program: RenderProgram<WasmProgramState> = {
    init(state) {
      state.custom.emu.reset();
    },
    update(state) {
      state.custom.emu.step();
    },
    render(state, surface) {
      const ptr = state.custom.emu.pixels_ptr();
      const len = state.custom.emu.pixels_len();
      const bytes = new Uint8Array(wasm.memory.buffer, ptr, len);
      surface.pixels.set(bytes.subarray(0, surface.pixels.length));
    }
  };

  return {
    initialState: { emu },
    program
  };
}
