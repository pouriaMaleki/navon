import type { AnnotationsMap } from "mobx";
import type { MinimapWasmEmulator } from "../wasm-pkg/render_core_wasm.js";

export type StoreAnnotations<T extends object> = AnnotationsMap<T, never>;

export type GesturePoint = {
  x: number;
  y: number;
};

export type GeoCoordinates = {
  lat: number;
  lon: number;
};

export type BikeControlIntent = {
  accelerate: boolean;
  brake: boolean;
  turnLeft: boolean;
  turnRight: boolean;
};

export type BikePhysicsState = {
  lat: number;
  lon: number;
  headingRad: number;
  speedMps: number;
  steerAngleRad: number;
};

export type BikePhysicsAdvanceResult = {
  state: BikePhysicsState;
  simDtSec: number;
  substeps: number;
  droppedDtSec: number;
};

export type BikePhysicsConfig = {
  maxSpeedKmh: number;
  throttleAccelMps2: number;
  coastDecelMps2: number;
  brakeDecelMps2: number;
  maxSteerDeg: number;
  steerResponseDegPerSec: number;
  wheelbaseM: number;
};

export type SimulatedGeoSample = {
  lat: number;
  lon: number;
  headingRad: number;
  speedMps: number;
};

export type RuntimeGpsInput = {
  latDeg: number;
  lonDeg: number;
  speedMps: number;
  courseRad: number | null;
  horizontalAccuracyM: number | null;
};

export type RawTouchPhase = "started" | "moved" | "stationary" | "ended" | "cancelled";

export type RawTouchContact = {
  id: number;
  phase: RawTouchPhase;
  xPx: number;
  yPx: number;
  pressure: number | null;
};

export type RuntimeTouchInput = {
  sequence: number;
  contacts: RawTouchContact[];
};

export type RuntimeFrameState = {
  frameIndex: number;
  cameraMode: "riding" | "stopped";
  zoom: number;
  orientationRad: number;
  followLocked: boolean;
  recenterActive: boolean;
  northUpActive: boolean;
  geometryCount: number;
};

export type WasmRuntimeState = {
  emu: MinimapWasmEmulator;
  gps: RuntimeGpsInput | null;
  touch: RuntimeTouchInput;
  frame: RuntimeFrameState | null;
};
