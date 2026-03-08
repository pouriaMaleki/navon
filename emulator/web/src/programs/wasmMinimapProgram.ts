import type { EmulatorState, RenderProgram } from "../core/types";
import initWasm, {
  MinimapWasmEmulator,
  type InitOutput
} from "../../wasm-pkg/render_core_wasm.js";

import type { FrameBuffer } from "../core/framebuffer";

export type WasmProgramState = {
  emu: MinimapWasmEmulator;
  hasGeo: boolean;
  lat: number;
  lon: number;
  headingRad: number;
  zoom: number;
  panX: number;
  panY: number;
  lastPanInputMs: number;
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
      state.custom.zoom = 2.2;
      state.custom.panX = 0;
      state.custom.panY = 0;
      state.custom.lastPanInputMs = 0;
    },
    update(state) {
      if (state.custom.hasGeo) {
        state.custom.emu.set_user_geo(
          state.custom.lat,
          state.custom.lon,
          state.custom.headingRad
        );
      }
      const idleMs = state.time.totalMs - state.custom.lastPanInputMs;
      if (idleMs > 1200) {
        const t = Math.min(1, state.time.dtMs / 420);
        state.custom.panX *= 1 - t;
        state.custom.panY *= 1 - t;
      }
      state.custom.emu.set_camera(state.custom.zoom, state.custom.panX, state.custom.panY);
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
    initialState: {
      emu,
      hasGeo: false,
      lat: 0,
      lon: 0,
      headingRad: 0,
      zoom: 2.2,
      panX: 0,
      panY: 0,
      lastPanInputMs: 0
    },
    program
  };
}
