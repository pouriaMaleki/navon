import type { RenderProgram } from "../core/types";

import type { RuntimeGpsInput, RuntimeTouchInput, WasmRuntimeState } from "../types";

const SPEED_UNIT_STORAGE_KEY = "esp32-minimap.speed-unit";
const DEG_LAT_PER_M = 1 / 111_320;

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

function buildDemoRouteSync(gps: RuntimeGpsInput): RuntimeRouteSyncInput {
  const start = { latDeg: gps.latDeg, lonDeg: gps.lonDeg };
  const p1 = offsetLatLon(gps.latDeg, gps.lonDeg, 120, 20);
  const p2 = offsetLatLon(gps.latDeg, gps.lonDeg, 240, 110);
  const p3 = offsetLatLon(gps.latDeg, gps.lonDeg, 330, 190);
  const routeId = `demo-${Math.round(gps.latDeg * 1e5)}-${Math.round(gps.lonDeg * 1e5)}`;

  return {
    type: "set",
    route: {
      version: { major: 1, minor: 0 },
      routeId,
      revision: 1,
      geometry: [start, p1, p2, p3],
      maneuvers: [
        {
          id: "depart",
          maneuverType: "depart",
          location: start,
          distanceFromStartM: 0,
          distanceToNextM: 140,
          instructionText: "Start riding",
        },
        {
          id: "turn",
          maneuverType: "right",
          location: p1,
          distanceFromStartM: 140,
          distanceToNextM: 220,
          instructionText: "Turn slight right",
        },
        {
          id: "arrive",
          maneuverType: "arrive",
          location: p3,
          distanceFromStartM: 360,
          distanceToNextM: null,
          instructionText: "Arrive",
        },
      ],
      summary: {
        totalDistanceM: 360,
        estimatedDurationS: 130,
        startLabel: "Start",
        destinationLabel: "Demo Destination",
      },
      provenance: {
        provider: "hsl_digitransit",
        sourceRef: "emulator-demo-route",
        generatedAtUnixMs: Date.now(),
      },
    },
  };
}

function offsetLatLon(
  latDeg: number,
  lonDeg: number,
  northMeters: number,
  eastMeters: number,
): { latDeg: number; lonDeg: number } {
  const latDelta = northMeters * DEG_LAT_PER_M;
  const lonScale = Math.max(0.2, Math.cos((latDeg * Math.PI) / 180));
  const lonDelta = eastMeters * (DEG_LAT_PER_M / lonScale);
  return {
    latDeg: latDeg + latDelta,
    lonDeg: lonDeg + lonDelta,
  };
}
