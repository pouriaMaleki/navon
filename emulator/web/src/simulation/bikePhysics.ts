import type {
  BikeControlIntent,
  BikePhysicsAdvanceResult,
  BikePhysicsConfig,
  BikePhysicsState,
  SimulatedGeoSample,
} from "../types";

const EARTH_RADIUS_M = 6_371_000;
export const MAX_SUBSTEP_SEC = 0.05;
export const MAX_SIM_PER_FRAME_SEC = 0.5;
const PHYSICS_DEBUG_INTERVAL_MS = 1200;

let lastPhysicsDebugMs = 0;

export const DEFAULT_BIKE_PHYSICS_CONFIG: BikePhysicsConfig = {
  maxSpeedKmh: 30,
  throttleAccelMps2: 1.9,
  coastDecelMps2: 0.34,
  brakeDecelMps2: 1.45,
  maxSteerDeg: 13,
  steerResponseDegPerSec: 18,
  wheelbaseM: 1.04,
};

export function createInitialBikePhysicsState(lat: number, lon: number): BikePhysicsState {
  return {
    lat,
    lon,
    headingRad: 0,
    speedMps: 0,
    steerAngleRad: 0,
  };
}

export function stepBikePhysics(
  prev: BikePhysicsState,
  input: BikeControlIntent,
  dtSec: number,
  config: BikePhysicsConfig = DEFAULT_BIKE_PHYSICS_CONFIG,
): BikePhysicsState {
  const safeDtSec = Math.max(0, dtSec);
  const maxSpeedMps = Math.max(0, config.maxSpeedKmh / 3.6);
  const maxSteerRad = (config.maxSteerDeg * Math.PI) / 180;
  const steerResponseRadPerSec = (config.steerResponseDegPerSec * Math.PI) / 180;
  let speedMps = prev.speedMps;

  if (input.accelerate) {
    speedMps += config.throttleAccelMps2 * safeDtSec;
  } else if (input.brake) {
    speedMps -= config.brakeDecelMps2 * safeDtSec;
  } else {
    speedMps -= config.coastDecelMps2 * safeDtSec;
  }
  speedMps = clamp(speedMps, 0, maxSpeedMps);

  const steerTargetRad =
    input.turnLeft && !input.turnRight
      ? -maxSteerRad
      : input.turnRight && !input.turnLeft
        ? maxSteerRad
        : 0;
  const steerAngleRad = approach(
    prev.steerAngleRad,
    steerTargetRad,
    steerResponseRadPerSec * safeDtSec,
  );

  const turnRateRadPerSec =
    speedMps > 0.05 ? (speedMps / Math.max(0.2, config.wheelbaseM)) * Math.tan(steerAngleRad) : 0;
  const headingRad = normalizeHeading(prev.headingRad + turnRateRadPerSec * safeDtSec);

  const distanceM = speedMps * safeDtSec;
  const northM = Math.cos(headingRad) * distanceM;
  const eastM = Math.sin(headingRad) * distanceM;

  const latRad = (prev.lat * Math.PI) / 180;
  const nextLat = prev.lat + (northM / EARTH_RADIUS_M) * (180 / Math.PI);
  const lonScale = Math.max(0.00001, Math.cos(latRad));
  const nextLon = prev.lon + (eastM / (EARTH_RADIUS_M * lonScale)) * (180 / Math.PI);

  const nextState: BikePhysicsState = {
    lat: nextLat,
    lon: nextLon,
    headingRad,
    speedMps,
    steerAngleRad,
  };
  logBikePhysicsFrame(
    prev,
    nextState,
    input,
    safeDtSec,
    steerTargetRad,
    turnRateRadPerSec,
    distanceM,
    northM,
    eastM,
  );
  return nextState;
}

export function advanceBikePhysics(
  prev: BikePhysicsState,
  input: BikeControlIntent,
  elapsedDtSec: number,
  config: BikePhysicsConfig = DEFAULT_BIKE_PHYSICS_CONFIG,
): BikePhysicsAdvanceResult {
  const safeElapsedDtSec = Math.max(0, elapsedDtSec);
  const simDtSec = Math.min(safeElapsedDtSec, MAX_SIM_PER_FRAME_SEC);
  const droppedDtSec = Math.max(0, safeElapsedDtSec - simDtSec);
  if (simDtSec <= 0) {
    return {
      state: prev,
      simDtSec,
      substeps: 0,
      droppedDtSec,
    };
  }

  const substeps = Math.max(1, Math.ceil(simDtSec / MAX_SUBSTEP_SEC));
  const substepDtSec = simDtSec / substeps;
  let state = prev;
  for (let i = 0; i < substeps; i += 1) {
    state = stepBikePhysics(state, input, substepDtSec, config);
  }

  return {
    state,
    simDtSec,
    substeps,
    droppedDtSec,
  };
}

export function toSimulatedGeoSample(state: BikePhysicsState): SimulatedGeoSample {
  return {
    lat: state.lat,
    lon: state.lon,
    headingRad: state.headingRad,
    speedMps: state.speedMps,
  };
}

export function measureGroundSpeedKmh(
  prevLat: number,
  prevLon: number,
  nextLat: number,
  nextLon: number,
  wallDtSec: number,
): number {
  const safeWallDtSec = Math.max(0, wallDtSec);
  if (safeWallDtSec <= 0) {
    return 0;
  }
  const distM = haversineMeters(prevLat, prevLon, nextLat, nextLon);
  return (distM / safeWallDtSec) * 3.6;
}

function approach(current: number, target: number, maxStep: number): number {
  if (current < target) {
    return Math.min(current + maxStep, target);
  }
  return Math.max(current - maxStep, target);
}

function clamp(value: number, min: number, max: number): number {
  return Math.max(min, Math.min(max, value));
}

function normalizeHeading(value: number): number {
  const twoPi = Math.PI * 2;
  let out = value % twoPi;
  if (out < 0) {
    out += twoPi;
  }
  return out;
}

function logBikePhysicsFrame(
  prev: BikePhysicsState,
  next: BikePhysicsState,
  input: BikeControlIntent,
  dtSec: number,
  steerTargetRad: number,
  turnRateRadPerSec: number,
  distanceM: number,
  northM: number,
  eastM: number,
): void {
  if (
    !input.accelerate &&
    !input.brake &&
    !input.turnLeft &&
    !input.turnRight &&
    next.speedMps < 0.15
  ) {
    return;
  }
  const nowMs = typeof performance !== "undefined" ? performance.now() : Date.now();
  if (nowMs - lastPhysicsDebugMs < PHYSICS_DEBUG_INTERVAL_MS) {
    return;
  }
  lastPhysicsDebugMs = nowMs;

  console.info("[emu:bike:physics] step", {
    dtMs: Number((dtSec * 1000).toFixed(1)),
    input: { ...input },
    speedKmh: Number((next.speedMps * 3.6).toFixed(2)),
    steerDeg: Number(((next.steerAngleRad * 180) / Math.PI).toFixed(2)),
    steerTargetDeg: Number(((steerTargetRad * 180) / Math.PI).toFixed(2)),
    turnRateDegPerSec: Number(((turnRateRadPerSec * 180) / Math.PI).toFixed(2)),
    prevHeadingDeg: Number(((prev.headingRad * 180) / Math.PI).toFixed(2)),
    headingDeg: Number(((next.headingRad * 180) / Math.PI).toFixed(2)),
    headingDeltaDeg: Number(
      ((signedAngleDelta(next.headingRad, prev.headingRad) * 180) / Math.PI).toFixed(3),
    ),
    distanceM: Number(distanceM.toFixed(3)),
    northM: Number(northM.toFixed(3)),
    eastM: Number(eastM.toFixed(3)),
    lat: Number(next.lat.toFixed(6)),
    lon: Number(next.lon.toFixed(6)),
  });
}

function signedAngleDelta(currentRad: number, prevRad: number): number {
  const twoPi = Math.PI * 2;
  let delta = (currentRad - prevRad) % twoPi;
  if (delta > Math.PI) {
    delta -= twoPi;
  } else if (delta < -Math.PI) {
    delta += twoPi;
  }
  return delta;
}

function haversineMeters(lat1: number, lon1: number, lat2: number, lon2: number): number {
  const p1 = (lat1 * Math.PI) / 180;
  const p2 = (lat2 * Math.PI) / 180;
  const dLat = p2 - p1;
  const dLon = ((lon2 - lon1) * Math.PI) / 180;
  const a =
    Math.sin(dLat * 0.5) * Math.sin(dLat * 0.5) +
    Math.cos(p1) * Math.cos(p2) * Math.sin(dLon * 0.5) * Math.sin(dLon * 0.5);
  return 2 * EARTH_RADIUS_M * Math.asin(Math.min(1, Math.sqrt(a)));
}
