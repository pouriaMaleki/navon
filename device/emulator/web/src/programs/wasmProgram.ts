import type { RenderProgram } from "../core/types";

import type {
  RouteAlertVerbosity,
  RuntimeGpsInput,
  RuntimeRouteManeuverType,
  RuntimeRoutePackageInput,
  RuntimeRoutePointInput,
  RuntimeRouteSyncInput,
  RuntimeTouchInput,
  WasmRuntimeState,
} from "../types";

const SPEED_UNIT_STORAGE_KEY = "esp32-minimap.speed-unit";

const DEMO_ROUTE_ID = "demo-helsinki-zigzag";
const DEMO_ROUTE_REVISION = 2;
const DEMO_REROUTE_REVISION = 3;
const DEMO_REROUTE_DELAY_MS = 900;
const EARTH_RADIUS_M = 6_371_000;
const DEMO_ROUTE_GEOMETRY: RuntimeRoutePointInput[] = [
  { latDeg: 60.17442, lonDeg: 24.9421 },
  { latDeg: 60.17495, lonDeg: 24.94208 },
  { latDeg: 60.17497, lonDeg: 24.94262 },
  { latDeg: 60.17524, lonDeg: 24.94264 },
  { latDeg: 60.17525, lonDeg: 24.94228 },
  { latDeg: 60.17555, lonDeg: 24.9423 },
  { latDeg: 60.17556, lonDeg: 24.94288 },
  { latDeg: 60.17582, lonDeg: 24.9429 },
  { latDeg: 60.17584, lonDeg: 24.94246 },
  { latDeg: 60.1761, lonDeg: 24.94248 },
];

type DemoManeuverDefinition = {
  id: string;
  geometryIndex: number;
  maneuverType?: RuntimeRouteManeuverType;
  instructionText: string | null;
};

export async function createWasmProgram(
  profileId = 0,
  options?: { routeAlertVerbosity?: RouteAlertVerbosity },
): Promise<{
  initialState: WasmRuntimeState;
  program: RenderProgram<WasmRuntimeState>;
}> {
  const { default: initWasm, MinimapWasmEmulator } = await import(
    "../../wasm-pkg/render_core_wasm.js"
  );
  type InitOutput = Awaited<ReturnType<typeof initWasm>>;
  const wasm: InitOutput = await initWasm();
  const emu = new MinimapWasmEmulator(
    profileId,
    readStoredSpeedUnit(globalThis.localStorage),
    options?.routeAlertVerbosity ?? readRouteAlertVerbosity(globalThis.location?.search),
  );

  const program: RenderProgram<WasmRuntimeState> = {
    init(state) {
      state.custom.emu.reset();
      state.custom.frame = null;
      state.custom.activeTouch = null;
      state.custom.pendingTouchFrames = [];
      state.custom.routeSeeded = false;
      state.custom.rerouteApplied = false;
      state.custom.reroutePendingSinceMs = null;
      state.custom.queuedRouteSync = null;
    },
    update(state) {
      const steps = consumeTouchFrames(state.custom, Math.max(0, state.time.dtMs));
      const gpsSample = state.custom.gps;
      state.custom.gps = null;

      const queuedRouteSync = state.custom.queuedRouteSync;
      if (queuedRouteSync) {
        state.custom.queuedRouteSync = null;
        state.custom.routeSeeded = true;
        state.custom.rerouteApplied = false;
        state.custom.reroutePendingSinceMs = null;
      }

      const routeSync = queuedRouteSync ?? maybeBuildRouteSync(state, gpsSample);

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
      rerouteApplied: false,
      reroutePendingSinceMs: null,
      queuedRouteSync: null,
    },
    program,
  };
}

export async function importGpxRouteSyncFromFile(file: Pick<File, "text" | "name">): Promise<RuntimeRouteSyncInput> {
  const gpxXml = await file.text();
  return importGpxRouteSyncFromText(gpxXml, file.name);
}

export async function importGpxRouteSyncFromText(
  gpxXml: string,
  sourceRef?: string | null,
): Promise<RuntimeRouteSyncInput> {
  const { default: initWasm, import_gpx_route_package } = await import(
    "../../wasm-pkg/render_core_wasm.js"
  );
  await initWasm();
  const routeJson = import_gpx_route_package(gpxXml, sourceRef ?? null);
  return {
    type: "set",
    route: JSON.parse(routeJson) as RuntimeRoutePackageInput,
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

export function readRouteAlertVerbosity(
  search: string | null | undefined,
): RouteAlertVerbosity | undefined {
  const raw = typeof search === "string" ? search : "";
  const value = new URLSearchParams(raw).get("routeAlerts")?.trim().toLowerCase() ?? null;
  if (value === "essential" || value === "standard" || value === "detailed") {
    return value;
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

export function maybeBuildRouteSync(
  state: { custom: WasmRuntimeState; time: { totalMs: number } },
  gpsSample: RuntimeGpsInput | null,
): RuntimeRouteSyncInput | null {
  if (!gpsSample) {
    return null;
  }

  if (!state.custom.routeSeeded) {
    state.custom.routeSeeded = true;
    return buildDemoRouteSync();
  }

  if (!state.custom.frame?.routeRerouteRequested) {
    state.custom.reroutePendingSinceMs = null;
    state.custom.rerouteApplied = false;
    return null;
  }

  if (state.custom.rerouteApplied) {
    return null;
  }

  if (state.custom.reroutePendingSinceMs == null) {
    state.custom.reroutePendingSinceMs = state.time.totalMs;
    return null;
  }

  if (state.time.totalMs - state.custom.reroutePendingSinceMs < DEMO_REROUTE_DELAY_MS) {
    return null;
  }

  state.custom.rerouteApplied = true;
  state.custom.reroutePendingSinceMs = null;
  return buildDemoRerouteSync(gpsSample);
}

export function buildDemoRouteSync(): RuntimeRouteSyncInput {
  const geometry = DEMO_ROUTE_GEOMETRY;

  return {
    type: "set",
    route: buildRoutePackage({
      routeId: DEMO_ROUTE_ID,
      revision: DEMO_ROUTE_REVISION,
      geometry,
      maneuvers: [
        {
          id: "depart",
          geometryIndex: 0,
          maneuverType: "depart",
          instructionText: "Roll north out of the start",
        },
        {
          id: "right-1",
          geometryIndex: 1,
          instructionText: "Quick right",
        },
        {
          id: "left-1",
          geometryIndex: 2,
          instructionText: "Immediate left",
        },
        {
          id: "left-2",
          geometryIndex: 3,
          instructionText: "Short left jog",
        },
        {
          id: "right-2",
          geometryIndex: 4,
          instructionText: "Snap back right",
        },
        {
          id: "right-3",
          geometryIndex: 5,
          instructionText: "Another quick right",
        },
        {
          id: "left-3",
          geometryIndex: 6,
          instructionText: "Immediate left again",
        },
        {
          id: "left-4",
          geometryIndex: 7,
          instructionText: "Tight left",
        },
        {
          id: "right-4",
          geometryIndex: 8,
          instructionText: "Finish with a right kink",
        },
        {
          id: "arrive",
          geometryIndex: 9,
          maneuverType: "arrive",
          instructionText: "Arrive",
        },
      ],
      estimatedDurationS: 105,
      startLabel: "Helsinki Demo Start",
      destinationLabel: "Helsinki Zigzag Finish",
      sourceRef: "emulator-helsinki-zigzag",
    }),
  };
}

export function buildDemoRerouteSync(gps: RuntimeGpsInput): RuntimeRouteSyncInput {
  const geometry = buildDemoRerouteGeometry(gps);

  return {
    type: "update",
    routeId: DEMO_ROUTE_ID,
    revision: DEMO_REROUTE_REVISION,
    route: buildRoutePackage({
      routeId: DEMO_ROUTE_ID,
      revision: DEMO_REROUTE_REVISION,
      geometry,
      maneuvers: buildDemoRerouteManeuvers(geometry),
      estimatedDurationS: 90,
      startLabel: "Current Position",
      destinationLabel: "Helsinki Zigzag Finish",
      sourceRef: "emulator-helsinki-reroute",
    }),
  };
}

export function buildDemoRerouteGeometry(gps: RuntimeGpsInput): RuntimeRoutePointInput[] {
  const start = { latDeg: gps.latDeg, lonDeg: gps.lonDeg };
  const rejoinGeometryIndex = computeRejoinGeometryIndex(DEMO_ROUTE_GEOMETRY, start);
  const remainingGeometry = DEMO_ROUTE_GEOMETRY.slice(rejoinGeometryIndex);
  if (remainingGeometry.length === 0) {
    return [start, DEMO_ROUTE_GEOMETRY[DEMO_ROUTE_GEOMETRY.length - 1]!];
  }
  if (
    remainingGeometry.length === 1 &&
    remainingGeometry[0]?.latDeg === start.latDeg &&
    remainingGeometry[0]?.lonDeg === start.lonDeg
  ) {
    return [start, DEMO_ROUTE_GEOMETRY[DEMO_ROUTE_GEOMETRY.length - 1]!];
  }
  return [start, ...remainingGeometry];
}

function buildDemoRerouteManeuvers(geometry: RuntimeRoutePointInput[]): DemoManeuverDefinition[] {
  const maneuvers: DemoManeuverDefinition[] = [
    {
      id: "depart-reroute",
      geometryIndex: 0,
      maneuverType: "depart",
      instructionText: "Rejoin from your current position",
    },
  ];

  for (let geometryIndex = 1; geometryIndex < geometry.length - 1; geometryIndex += 1) {
    const maneuverType = inferTurnManeuverType(geometry, geometryIndex);
    if (maneuverType === "straight") {
      continue;
    }
    maneuvers.push({
      id: `reroute-turn-${geometryIndex}`,
      geometryIndex,
      maneuverType,
      instructionText:
        geometryIndex === 1 ? "Rejoin the route ahead" : "Stay on the reroute toward the finish",
    });
  }

  maneuvers.push({
    id: "arrive-reroute",
    geometryIndex: geometry.length - 1,
    maneuverType: "arrive",
    instructionText: "Back on track",
  });

  return maneuvers;
}

export function computeRejoinGeometryIndex(
  geometry: RuntimeRoutePointInput[],
  point: RuntimeRoutePointInput,
): number {
  if (geometry.length < 2) {
    return 0;
  }

  let bestSegmentStartIndex = 0;
  let bestT = 0;
  let bestDistanceSq = Number.POSITIVE_INFINITY;

  for (let index = 0; index < geometry.length - 1; index += 1) {
    const start = geometry[index];
    const end = geometry[index + 1];
    if (!start || !end) {
      continue;
    }
    const { distanceSqM, t } = projectPointOntoSegmentMeters(point, start, end);
    if (distanceSqM < bestDistanceSq) {
      bestDistanceSq = distanceSqM;
      bestSegmentStartIndex = index;
      bestT = t;
    }
  }

  const rejoinIndex = bestT >= 0.5 ? bestSegmentStartIndex + 1 : bestSegmentStartIndex;
  return Math.min(Math.max(rejoinIndex, 0), geometry.length - 1);
}

function projectPointOntoSegmentMeters(
  point: RuntimeRoutePointInput,
  start: RuntimeRoutePointInput,
  end: RuntimeRoutePointInput,
): { distanceSqM: number; t: number } {
  const segment = localSegmentVectorMeters(start, end);
  const pointFromStart = localSegmentVectorMeters(start, point);
  const segmentLengthSq = segment.dxM * segment.dxM + segment.dyM * segment.dyM;
  if (segmentLengthSq <= Number.EPSILON) {
    return {
      distanceSqM: pointFromStart.dxM * pointFromStart.dxM + pointFromStart.dyM * pointFromStart.dyM,
      t: 0,
    };
  }

  const unclampedT =
    (pointFromStart.dxM * segment.dxM + pointFromStart.dyM * segment.dyM) / segmentLengthSq;
  const t = Math.min(1, Math.max(0, unclampedT));
  const projectedDxM = segment.dxM * t;
  const projectedDyM = segment.dyM * t;
  const deltaDxM = pointFromStart.dxM - projectedDxM;
  const deltaDyM = pointFromStart.dyM - projectedDyM;
  return {
    distanceSqM: deltaDxM * deltaDxM + deltaDyM * deltaDyM,
    t,
  };
}

function buildRoutePackage(options: {
  routeId: string;
  revision: number;
  geometry: RuntimeRoutePointInput[];
  maneuvers: DemoManeuverDefinition[];
  estimatedDurationS: number;
  startLabel: string | null;
  destinationLabel: string | null;
  sourceRef: string;
}): RuntimeRoutePackageInput {
  const cumulativeDistances = computeCumulativeDistances(options.geometry);
  const totalDistanceM = roundMeters(cumulativeDistances.at(-1) ?? 0);
  validateDemoTurnCoverage(options.geometry, options.maneuvers, options.routeId);

  return {
    version: { major: 1, minor: 0 },
    routeId: options.routeId,
    revision: options.revision,
    geometry: options.geometry,
    maneuvers: options.maneuvers.map((maneuver, index) => {
      const location = options.geometry[maneuver.geometryIndex];
      const distanceFromStart = cumulativeDistances[maneuver.geometryIndex];
      if (!location || distanceFromStart == null) {
        throw new Error(`Invalid demo maneuver geometry index: ${maneuver.geometryIndex}`);
      }

      const maneuverType =
        maneuver.maneuverType ?? inferTurnManeuverType(options.geometry, maneuver.geometryIndex);
      const nextGeometryIndex = options.maneuvers[index + 1]?.geometryIndex;
      const nextDistanceFromStart =
        typeof nextGeometryIndex === "number" ? cumulativeDistances[nextGeometryIndex] : null;
      const distanceToNextM =
        nextDistanceFromStart == null
          ? null
          : roundMeters(Math.max(0, nextDistanceFromStart - distanceFromStart));
      return {
        id: maneuver.id,
        maneuverType,
        location,
        distanceFromStartM: roundMeters(distanceFromStart),
        distanceToNextM,
        instructionText: maneuver.instructionText,
      };
    }),
    summary: {
      totalDistanceM,
      estimatedDurationS: options.estimatedDurationS,
      startLabel: options.startLabel,
      destinationLabel: options.destinationLabel,
    },
    provenance: {
      provider: "hsl_digitransit",
      sourceRef: options.sourceRef,
      generatedAtUnixMs: Date.now(),
    },
  };
}

export function inferTurnManeuverType(
  geometry: RuntimeRoutePointInput[],
  geometryIndex: number,
): RuntimeRouteManeuverType {
  if (geometryIndex <= 0 || geometryIndex >= geometry.length - 1) {
    throw new Error(`Cannot infer turn maneuver type at geometry index ${geometryIndex}`);
  }

  const previous = geometry[geometryIndex - 1];
  const current = geometry[geometryIndex];
  const next = geometry[geometryIndex + 1];
  if (!previous || !current || !next) {
    throw new Error(`Missing geometry around maneuver index ${geometryIndex}`);
  }

  const incoming = localSegmentVectorMeters(previous, current);
  const outgoing = localSegmentVectorMeters(current, next);
  const incomingLength = Math.hypot(incoming.dxM, incoming.dyM);
  const outgoingLength = Math.hypot(outgoing.dxM, outgoing.dyM);
  if (incomingLength <= Number.EPSILON || outgoingLength <= Number.EPSILON) {
    return "straight";
  }

  const signedAngleRad = Math.atan2(
    incoming.dxM * outgoing.dyM - incoming.dyM * outgoing.dxM,
    incoming.dxM * outgoing.dxM + incoming.dyM * outgoing.dyM,
  );
  const absoluteAngleRad = Math.abs(signedAngleRad);
  if (absoluteAngleRad >= 2.35) {
    return "uturn";
  }
  if (absoluteAngleRad < 0.35) {
    return "straight";
  }
  if (absoluteAngleRad >= 1.6) {
    return signedAngleRad > 0 ? "sharp_left" : "sharp_right";
  }
  if (absoluteAngleRad < 0.75) {
    return signedAngleRad > 0 ? "slight_left" : "slight_right";
  }
  return signedAngleRad > 0 ? "left" : "right";
}

function significantTurnGeometryIndexes(geometry: RuntimeRoutePointInput[]): number[] {
  const indexes: number[] = [];
  for (let geometryIndex = 1; geometryIndex < geometry.length - 1; geometryIndex += 1) {
    const maneuverType = inferTurnManeuverType(geometry, geometryIndex);
    if (maneuverType !== "straight") {
      indexes.push(geometryIndex);
    }
  }
  return indexes;
}

function validateDemoTurnCoverage(
  geometry: RuntimeRoutePointInput[],
  maneuvers: DemoManeuverDefinition[],
  routeId: string,
): void {
  const expected = significantTurnGeometryIndexes(geometry);
  const declared = maneuvers
    .filter((maneuver) => maneuver.geometryIndex > 0 && maneuver.geometryIndex < geometry.length - 1)
    .map((maneuver) => maneuver.geometryIndex);
  const missing = expected.filter((geometryIndex) => !declared.includes(geometryIndex));
  const unexpected = declared.filter((geometryIndex) => !expected.includes(geometryIndex));
  if (missing.length > 0 || unexpected.length > 0) {
    throw new Error(
      `Demo route ${routeId} turn coverage mismatch: missing [${missing.join(", ")}], unexpected [${unexpected.join(", ")}]`,
    );
  }
}

function localSegmentVectorMeters(
  from: RuntimeRoutePointInput,
  to: RuntimeRoutePointInput,
): { dxM: number; dyM: number } {
  const fromLatRad = degreesToRadians(from.latDeg);
  const toLatRad = degreesToRadians(to.latDeg);
  const meanLatRad = (fromLatRad + toLatRad) * 0.5;
  return {
    dxM: degreesToRadians(to.lonDeg - from.lonDeg) * Math.cos(meanLatRad) * EARTH_RADIUS_M,
    dyM: (toLatRad - fromLatRad) * EARTH_RADIUS_M,
  };
}

function computeCumulativeDistances(geometry: RuntimeRoutePointInput[]): number[] {
  const distances = [0];
  for (let index = 1; index < geometry.length; index += 1) {
    const previous = geometry[index - 1];
    const current = geometry[index];
    if (!previous || !current) {
      continue;
    }
    const previousDistance = distances[index - 1] ?? 0;
    distances.push(previousDistance + distanceMeters(previous, current));
  }
  return distances;
}

function distanceMeters(a: RuntimeRoutePointInput, b: RuntimeRoutePointInput): number {
  const lat1 = degreesToRadians(a.latDeg);
  const lat2 = degreesToRadians(b.latDeg);
  const dLat = lat2 - lat1;
  const dLon = degreesToRadians(b.lonDeg - a.lonDeg);
  const x = dLon * Math.cos((lat1 + lat2) * 0.5);
  return Math.sqrt(x * x + dLat * dLat) * EARTH_RADIUS_M;
}

function degreesToRadians(value: number): number {
  return (value * Math.PI) / 180;
}

function roundMeters(value: number): number {
  return Math.max(0, Math.round(value));
}
