import type { RenderProgram } from "../core/types";

import type { RuntimeTouchInput, WasmRuntimeState } from "../types";

const SPEED_UNIT_STORAGE_KEY = "esp32-minimap.speed-unit";

export async function createWasmProgram(profileId = 0): Promise<{
  initialState: WasmRuntimeState;
  program: RenderProgram<WasmRuntimeState>;
}> {
  const { default: initWasm, MinimapWasmEmulator } = await import(
    "../../wasm-pkg/render_core_wasm.js"
  );
  type InitOutput = Awaited<ReturnType<typeof initWasm>>;
  const wasm: InitOutput = await initWasm();
  const emu = new MinimapWasmEmulator(profileId, readStoredSpeedUnit(globalThis.localStorage));

  const program: RenderProgram<WasmRuntimeState> = {
    init(state) {
      state.custom.emu.reset();
      state.custom.frame = null;
      state.custom.activeTouch = null;
      state.custom.pendingTouchFrames = [];
    },
    update(state) {
      const steps = consumeTouchFrames(state.custom, Math.max(0, state.time.dtMs));
      let snapshotJson = "";
      for (const step of steps) {
        snapshotJson = state.custom.emu.step_frame(
          step.dtMs,
          JSON.stringify({
            viewport: {
              widthPx: state.profile.width,
              heightPx: state.profile.height,
            },
            gps: state.custom.gps,
            touch: step.touch,
          }),
        );
      }
      state.custom.frame = JSON.parse(snapshotJson) as NonNullable<WasmRuntimeState["frame"]>;
      persistSpeedUnit(globalThis.localStorage, state.custom.frame.speedUnit);
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
      gps: null,
      activeTouch: null,
      pendingTouchFrames: [],
      frame: null,
    },
    program,
  };
}

export function readStoredSpeedUnit(
  storage: Pick<Storage, "getItem"> | null | undefined,
): "kph" | "mph" | undefined {
  try {
    const value = storage?.getItem(SPEED_UNIT_STORAGE_KEY) ?? null;
    if (value === "kph" || value === "mph") {
      return value;
    }
  } catch {
    return undefined;
  }
  return undefined;
}

export function persistSpeedUnit(
  storage: Pick<Storage, "setItem"> | null | undefined,
  unit: "kph" | "mph",
): void {
  try {
    storage?.setItem(SPEED_UNIT_STORAGE_KEY, unit);
  } catch {
    // Ignore storage write failures so rendering remains uninterrupted.
  }
}

function consumeTouchFrames(
  state: WasmRuntimeState,
  dtMs: number,
): Array<{ dtMs: number; touch: RuntimeTouchInput | null }> {
  const queued = state.pendingTouchFrames.splice(0);
  if (queued.length === 0) {
    return [{ dtMs, touch: state.activeTouch }];
  }

  return queued.map((touch, index) => ({
    dtMs: index + 1 === queued.length ? dtMs : 0,
    touch,
  }));
}
