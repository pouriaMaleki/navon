import type { RenderProgram } from "../core/types";

import type { RuntimeGpsInput, RuntimeTouchInput, WasmRuntimeState } from "../types";

const SPEED_UNIT_STORAGE_KEY = "esp32-minimap.speed-unit";
type RuntimeRouteSyncInput =
  | {
      type: "set";
      route: RuntimeRoutePackageInput;
    }
  | {
      type: "update";
      routeId: string;
      revision: number;
      route: RuntimeRoutePackageInput;
    }
  | {
      type: "clear";
      routeId: string | null;
    };

type RuntimeRoutePackageInput = {
  version: { major: number; minor: number };
  routeId: string;
  revision: number;
  geometry: Array<{ latDeg: number; lonDeg: number }>;
  maneuvers: Array<{
    id: string;
    maneuverType:
      | "depart"
      | "straight"
      | "slight_left"
      | "left"
      | "sharp_left"
      | "slight_right"
      | "right"
      | "sharp_right"
      | "uturn"
      | "roundabout"
      | "merge"
      | "ramp"
      | "arrive";
    location: { latDeg: number; lonDeg: number };
    distanceFromStartM: number;
    distanceToNextM: number | null;
    instructionText: string | null;
  }>;
  summary: {
    totalDistanceM: number;
    estimatedDurationS: number;
    startLabel: string | null;
    destinationLabel: string | null;
  };
  provenance: {
    provider: "hsl_digitransit";
    sourceRef: string;
    generatedAtUnixMs: number;
  };
};

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
      state.custom.routeSeeded = false;
    },
    update(state) {
      const steps = consumeTouchFrames(state.custom, Math.max(0, state.time.dtMs));
      const gpsSample = state.custom.gps;
      state.custom.gps = null;

      const routeSync = !state.custom.routeSeeded && gpsSample ? buildDemoRouteSync(gpsSample) : null;
      if (routeSync) {
        state.custom.routeSeeded = true;
      }

      let snapshotJson = "";
      const gpsStepIndex = Math.max(0, steps.length - 1);
      for (const [index, step] of steps.entries()) {
        snapshotJson = state.custom.emu.step_frame(
          step.dtMs,
          JSON.stringify({
            viewport: {
              widthPx: state.profile.width,
              heightPx: state.profile.height,
            },
            gps: index === gpsStepIndex ? gpsSample : null,
            touch: step.touch,
            routeSync: index === gpsStepIndex ? routeSync : null,
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
      routeSeeded: false,
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

function buildDemoRouteSync(_gps: RuntimeGpsInput): RuntimeRouteSyncInput {
  const start = { latDeg: 60.17442, lonDeg: 24.9421 };
  const p1 = { latDeg: 60.17495, lonDeg: 24.94208 };
  const p2 = { latDeg: 60.17497, lonDeg: 24.94262 };
  const p3 = { latDeg: 60.17524, lonDeg: 24.94264 };
  const p4 = { latDeg: 60.17525, lonDeg: 24.94228 };
  const p5 = { latDeg: 60.17555, lonDeg: 24.9423 };
  const p6 = { latDeg: 60.17556, lonDeg: 24.94288 };
  const p7 = { latDeg: 60.17582, lonDeg: 24.9429 };
  const p8 = { latDeg: 60.17584, lonDeg: 24.94246 };
  const finish = { latDeg: 60.1761, lonDeg: 24.94248 };
  const geometry = [start, p1, p2, p3, p4, p5, p6, p7, p8, finish];

  return {
    type: "set",
    route: {
      version: { major: 1, minor: 0 },
      routeId: "demo-helsinki-zigzag",
      revision: 2,
      geometry,
      maneuvers: [
        {
          id: "depart",
          maneuverType: "depart",
          location: start,
          distanceFromStartM: 0,
          distanceToNextM: 59,
          instructionText: "Roll north out of the start",
        },
        {
          id: "right-1",
          maneuverType: "right",
          location: p1,
          distanceFromStartM: 59,
          distanceToNextM: 30,
          instructionText: "Quick right",
        },
        {
          id: "left-1",
          maneuverType: "left",
          location: p2,
          distanceFromStartM: 89,
          distanceToNextM: 30,
          instructionText: "Immediate left",
        },
        {
          id: "left-2",
          maneuverType: "left",
          location: p3,
          distanceFromStartM: 119,
          distanceToNextM: 20,
          instructionText: "Short left jog",
        },
        {
          id: "right-2",
          maneuverType: "right",
          location: p4,
          distanceFromStartM: 139,
          distanceToNextM: 33,
          instructionText: "Snap back right",
        },
        {
          id: "right-3",
          maneuverType: "right",
          location: p5,
          distanceFromStartM: 172,
          distanceToNextM: 32,
          instructionText: "Another quick right",
        },
        {
          id: "left-3",
          maneuverType: "left",
          location: p6,
          distanceFromStartM: 204,
          distanceToNextM: 29,
          instructionText: "Immediate left again",
        },
        {
          id: "left-4",
          maneuverType: "left",
          location: p7,
          distanceFromStartM: 233,
          distanceToNextM: 24,
          instructionText: "Tight left",
        },
        {
          id: "right-4",
          maneuverType: "right",
          location: p8,
          distanceFromStartM: 258,
          distanceToNextM: 29,
          instructionText: "Finish with a right kink",
        },
        {
          id: "arrive",
          maneuverType: "arrive",
          location: finish,
          distanceFromStartM: 287,
          distanceToNextM: null,
          instructionText: "Arrive",
        },
      ],
      summary: {
        totalDistanceM: 287,
        estimatedDurationS: 105,
        startLabel: "Helsinki Demo Start",
        destinationLabel: "Helsinki Zigzag Finish",
      },
      provenance: {
        provider: "hsl_digitransit",
        sourceRef: "emulator-helsinki-zigzag",
        generatedAtUnixMs: Date.now(),
      },
    },
  };
}
