import initWasm, { type InitOutput, MinimapWasmEmulator } from "../../wasm-pkg/render_core_wasm.js";
import type { RenderProgram } from "../core/types";

import type { WasmRuntimeState } from "../types";

export const MIN_ZOOM = 0.9;
export const MAX_ZOOM = 20.0;
export const INITIAL_ZOOM = 2.2;

const PAN_IDLE_MS = 1200;
const PAN_RECENTER_MS = 420;

export async function createWasmProgram(profileId = 0): Promise<{
  initialState: WasmRuntimeState;
  program: RenderProgram<WasmRuntimeState>;
}> {
  const wasm: InitOutput = await initWasm();
  const emu = new MinimapWasmEmulator(profileId);

  const program: RenderProgram<WasmRuntimeState> = {
    init(state) {
      state.custom.emu.reset();
      state.custom.zoom = INITIAL_ZOOM;
      state.custom.panX = 0;
      state.custom.panY = 0;
      state.custom.lastPanInputMs = 0;
    },
    update(state) {
      if (state.custom.hasGeo) {
        state.custom.emu.set_user_geo(state.custom.lat, state.custom.lon, state.custom.headingRad);
      }

      const idleMs = performance.now() - state.custom.lastPanInputMs;
      if (idleMs > PAN_IDLE_MS) {
        const t = Math.min(1, state.time.dtMs / PAN_RECENTER_MS);
        state.custom.panX *= 1 - t;
        state.custom.panY *= 1 - t;
      }

      state.custom.emu.set_camera(
        state.custom.zoom,
        state.custom.panX,
        state.custom.panY,
        state.custom.headingRad,
        state.custom.headingRad,
        0.5,
        false,
        idleMs < 180,
      );
      state.custom.emu.step();
    },
    render(state, surface) {
      const ptr = state.custom.emu.pixels_ptr();
      const len = state.custom.emu.pixels_len();
      const bytes = new Uint8Array(wasm.memory.buffer, ptr, len);
      surface.pixels.set(bytes.subarray(0, surface.pixels.length));
    },
  };

  return {
    initialState: {
      emu,
      hasGeo: false,
      lat: 0,
      lon: 0,
      headingRad: 0,
      speedMps: 0,
      zoom: INITIAL_ZOOM,
      panX: 0,
      panY: 0,
      lastPanInputMs: 0,
    },
    program,
  };
}
