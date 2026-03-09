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

export type WasmRuntimeState = {
  emu: MinimapWasmEmulator;
  hasGeo: boolean;
  lat: number;
  lon: number;
  headingRad: number;
  speedMps: number;
  zoom: number;
  panX: number;
  panY: number;
  lastPanInputMs: number;
};
