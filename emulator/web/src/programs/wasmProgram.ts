import initWasm, { type InitOutput, MinimapWasmEmulator } from "../../wasm-pkg/render_core_wasm.js";
import type { RenderProgram } from "../core/types";

import type { WasmRuntimeState } from "../types";

export const MIN_ZOOM = 0.8;
export const MAX_ZOOM = 60.0;
export const INITIAL_ZOOM = 2.2;

const PAN_IDLE_MS = 1200;
const PAN_RECENTER_MS = 420;

export async function createWasmProgram(profileId = 0): Promise<{
  initialState: WasmRuntimeState;
  program: RenderProgram<WasmRuntimeState>;
}> {
  const wasm: InitOutput = await initWasm();
  const emu = new MinimapWasmEmulator(profileId);
  let prevPanX = 0;
  let prevPanY = 0;
  let prevZoom = INITIAL_ZOOM;

  const program: RenderProgram<WasmRuntimeState> = {
    init(state) {
      state.custom.emu.reset();
      state.custom.zoom = state.custom.emu.camera_zoom();
      state.custom.panX = 0;
      state.custom.panY = 0;
      state.custom.rotateDeltaRad = 0;
      state.custom.lastPanInputMs = 0;
      prevPanX = 0;
      prevPanY = 0;
      prevZoom = state.custom.zoom;
    },
    update(state) {
      if (state.custom.hasGeo) {
        state.custom.emu.set_user_geo(
          state.custom.lat,
          state.custom.lon,
          state.custom.headingRad,
          state.custom.speedMps,
        );
      }

      const idleMs = performance.now() - state.custom.lastPanInputMs;
      if (idleMs > PAN_IDLE_MS) {
        const t = Math.min(1, state.time.dtMs / PAN_RECENTER_MS);
        state.custom.panX *= 1 - t;
        state.custom.panY *= 1 - t;
      }

      const panDx = state.custom.panX - prevPanX;
      const panDy = state.custom.panY - prevPanY;
      const zoomScale = prevZoom > 0 ? state.custom.zoom / prevZoom : 1.0;
      const rotateDelta = state.custom.rotateDeltaRad;
      prevPanX = state.custom.panX;
      prevPanY = state.custom.panY;
      prevZoom = state.custom.zoom;
      state.custom.rotateDeltaRad = 0;

      state.custom.emu.set_gesture_deltas(panDx, panDy, zoomScale, rotateDelta, idleMs < 180);
      state.custom.emu.step(Math.max(0, state.time.dtMs));
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
      rotateDeltaRad: 0,
      lastPanInputMs: 0,
    },
    program,
  };
}
