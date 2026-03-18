import { makeAutoObservable } from "mobx";
import type { RuntimeGpsInput, SimulatedGeoSample, WasmRuntimeState } from "../types";

const DEFAULT_LAT = 60.17442;
const DEFAULT_LON = 24.9421;
const EARTH_RADIUS_M = 6_371_000;
const MOTION_DISTANCE_NOISE_M = 1.5;
const AUTO_REQUEST_GRACE_MS = 3_000;
const AUTO_REQUEST_TIMEOUT_MS = 4_000;
const USER_REQUEST_TIMEOUT_MS = 12_000;

type GpsMode =
  | "initializing"
  | "unsupported"
  | "insecure_context"
  | "simulated"
  | "requesting_auto"
  | "awaiting_user_gesture"
  | "requesting_user"
  | "live"
  | "denied"
  | "error";

export class GeoStore {
  mode: GpsMode = "initializing";
  isLive = false;

  private watchId: number | null = null;
  private autoRequestTimerId: number | null = null;
  private customState: WasmRuntimeState | null = null;
  private prevLat = 0;
  private prevLon = 0;
  private hasPrev = false;
  private prevTsMs = 0;
  private simulatedSample: SimulatedGeoSample = {
    lat: DEFAULT_LAT,
    lon: DEFAULT_LON,
    headingRad: 0,
    speedMps: 0,
  };

  constructor() {
    makeAutoObservable<
      GeoStore,
      | "watchId"
      | "autoRequestTimerId"
      | "customState"
      | "prevLat"
      | "prevLon"
      | "hasPrev"
      | "prevTsMs"
      | "simulatedSample"
    >(
      this,
      {
        watchId: false,
        autoRequestTimerId: false,
        customState: false,
        prevLat: false,
        prevLon: false,
        hasPrev: false,
        prevTsMs: false,
        simulatedSample: false,
      },
      { autoBind: true },
    );
  }

  bind(customState: WasmRuntimeState): void {
    this.customState = customState;
  }

  start(): void {
    if (!this.customState) {
      return;
    }
    this.activateSimulatedRuntimeSample();
    this.mode = "simulated";
    this.requestAutoLiveGps();
  }

  requestLiveGps(): void {
    this.requestUserLiveGps();
  }

  applySimulatedSample(sample: SimulatedGeoSample): void {
    this.simulatedSample = sample;
    if (this.isLive || !this.customState) {
      return;
    }
    this.writeGpsSample({
      latDeg: sample.lat,
      lonDeg: sample.lon,
      speedMps: sample.speedMps,
      courseRad: sample.headingRad,
      horizontalAccuracyM: null,
    });
  }

  dispose(): void {
    this.clearPendingGeolocation();
    this.customState = null;
    this.hasPrev = false;
    this.prevTsMs = 0;
    this.mode = "initializing";
    this.isLive = false;
  }

  get statusText(): string {
    switch (this.mode) {
      case "unsupported":
        return "GPS: unavailable (bike sim)";
      case "insecure_context":
        return "GPS: blocked (secure context required, bike sim)";
      case "requesting_auto":
        return "GPS: requesting permission";
      case "awaiting_user_gesture":
        return "GPS: tap Request GPS for live position";
      case "requesting_user":
        return "GPS: requesting permission";
      case "live":
        return "GPS: live";
      case "denied":
        return "GPS: denied (bike sim)";
      case "error":
        return "GPS: error (bike sim)";
      case "simulated":
        return "GPS: simulated (bike controls)";
      default:
        return "GPS: initializing";
    }
  }

  get requestButtonLabel(): string {
    switch (this.mode) {
      case "requesting_auto":
      case "requesting_user":
        return "Requesting GPS...";
      case "denied":
      case "error":
      case "awaiting_user_gesture":
        return "Retry GPS";
      default:
        return "Request GPS";
    }
  }

  get isRequestInFlight(): boolean {
    return this.mode === "requesting_auto" || this.mode === "requesting_user";
  }

  get needsUserAction(): boolean {
    return (
      this.mode === "awaiting_user_gesture" ||
      this.mode === "denied" ||
      this.mode === "error" ||
      this.mode === "insecure_context" ||
      this.mode === "unsupported"
    );
  }

  get statusTone(): "live" | "pending" | "attention" | "muted" {
    switch (this.mode) {
      case "live":
        return "live";
      case "requesting_auto":
      case "requesting_user":
        return "pending";
      case "awaiting_user_gesture":
      case "denied":
      case "error":
      case "insecure_context":
      case "unsupported":
        return "attention";
      default:
        return "muted";
    }
  }

  private activateSimulatedRuntimeSample(): void {
    if (!this.customState) {
      return;
    }
    this.isLive = false;
    this.writeGpsSample({
      latDeg: this.simulatedSample.lat,
      lonDeg: this.simulatedSample.lon,
      speedMps: this.simulatedSample.speedMps,
      courseRad: this.simulatedSample.headingRad,
      horizontalAccuracyM: null,
    });
  }

  private requestAutoLiveGps(): void {
    if (!this.canAttemptLiveGps()) {
      return;
    }

    this.clearPendingGeolocation();
    this.isLive = false;
    this.mode = "requesting_auto";
    this.autoRequestTimerId = window.setTimeout(() => {
      if (this.mode !== "requesting_auto" || this.isLive) {
        return;
      }
      this.clearWatch();
      this.mode = "awaiting_user_gesture";
    }, AUTO_REQUEST_GRACE_MS);

    navigator.geolocation.getCurrentPosition(
      (position) => {
        this.clearAutoRequestTimer();
        this.handleLivePosition(position);
        this.startLiveWatch();
      },
      (error) => {
        this.clearAutoRequestTimer();
        this.handleAutoRequestError(error);
      },
      {
        enableHighAccuracy: true,
        maximumAge: 1_000,
        timeout: AUTO_REQUEST_TIMEOUT_MS,
      },
    );
  }

  private requestUserLiveGps(): void {
    if (!this.canAttemptLiveGps()) {
      return;
    }

    this.clearPendingGeolocation();
    this.isLive = false;
    this.mode = "requesting_user";

    navigator.geolocation.getCurrentPosition(
      (position) => {
        this.handleLivePosition(position);
        this.startLiveWatch();
      },
      (error) => {
        this.handleUserRequestError(error);
      },
      {
        enableHighAccuracy: true,
        maximumAge: 0,
        timeout: USER_REQUEST_TIMEOUT_MS,
      },
    );
  }

  private canAttemptLiveGps(): boolean {
    if (!this.customState) {
      return false;
    }

    if (!("geolocation" in navigator)) {
      this.mode = "unsupported";
      return false;
    }

    if (!window.isSecureContext) {
      this.mode = "insecure_context";
      return false;
    }

    return true;
  }

  private startLiveWatch(): void {
    this.clearWatch();
    this.watchId = navigator.geolocation.watchPosition(
      (position) => {
        this.handleLivePosition(position);
      },
      (error) => {
        if (error.code === error.PERMISSION_DENIED) {
          this.activateSimulatedRuntimeSample();
          this.mode = "denied";
          return;
        }

        this.activateSimulatedRuntimeSample();
        this.mode = "error";
      },
      {
        enableHighAccuracy: true,
        maximumAge: 1_000,
        timeout: USER_REQUEST_TIMEOUT_MS,
      },
    );
  }

  private handleLivePosition(position: GeolocationPosition): void {
    if (!this.customState) {
      return;
    }

    const lat = position.coords.latitude;
    const lon = position.coords.longitude;
    const tsMs = position.timestamp;
    this.isLive = true;
    this.mode = "live";

    const sample = this.buildLiveGpsSample(position);
    this.writeGpsSample(sample);
    this.simulatedSample = {
      lat,
      lon,
      headingRad: sample.courseRad ?? this.simulatedSample.headingRad,
      speedMps: sample.speedMps,
    };

    this.prevLat = lat;
    this.prevLon = lon;
    this.prevTsMs = tsMs;
    this.hasPrev = true;
  }

  private handleAutoRequestError(error: GeolocationPositionError): void {
    this.activateSimulatedRuntimeSample();
    if (error.code === error.PERMISSION_DENIED) {
      this.mode = "denied";
      return;
    }

    this.mode = "awaiting_user_gesture";
  }

  private handleUserRequestError(error: GeolocationPositionError): void {
    this.activateSimulatedRuntimeSample();
    if (error.code === error.PERMISSION_DENIED) {
      this.mode = "denied";
      return;
    }

    this.mode = "error";
  }

  private clearPendingGeolocation(): void {
    this.clearAutoRequestTimer();
    this.clearWatch();
  }

  private clearAutoRequestTimer(): void {
    if (this.autoRequestTimerId === null) {
      return;
    }
    window.clearTimeout(this.autoRequestTimerId);
    this.autoRequestTimerId = null;
  }

  private clearWatch(): void {
    if (this.watchId === null || !("geolocation" in navigator)) {
      this.watchId = null;
      return;
    }
    navigator.geolocation.clearWatch(this.watchId);
    this.watchId = null;
  }

  private buildLiveGpsSample(position: GeolocationPosition): RuntimeGpsInput {
    const lat = position.coords.latitude;
    const lon = position.coords.longitude;
    const accuracyM = normalizeAccuracy(position.coords.accuracy);
    let distM = 0;
    let speedMps = 0;

    if (this.hasPrev) {
      distM = haversineMeters(this.prevLat, this.prevLon, lat, lon);
      if (distM > MOTION_DISTANCE_NOISE_M) {
        const dtS = Math.max(0.001, (position.timestamp - this.prevTsMs) / 1000);
        speedMps = distM / dtS;
      }
    }

    let headingDeg = normalizeHeading(position.coords.heading);
    if (headingDeg === null && this.hasPrev && distM > MOTION_DISTANCE_NOISE_M) {
      headingDeg = bearingDeg(this.prevLat, this.prevLon, lat, lon);
    }

    return {
      latDeg: lat,
      lonDeg: lon,
      speedMps,
      courseRad: headingDeg === null ? null : (headingDeg * Math.PI) / 180,
      horizontalAccuracyM: accuracyM,
    };
  }

  private writeGpsSample(sample: RuntimeGpsInput): void {
    if (!this.customState) {
      return;
    }
    this.customState.gps = sample;
  }
}

function normalizeHeading(headingDeg: number | null): number | null {
  if (headingDeg === null || Number.isNaN(headingDeg) || !Number.isFinite(headingDeg)) {
    return null;
  }
  return headingDeg;
}

function normalizeAccuracy(accuracyM: number): number | null {
  if (Number.isNaN(accuracyM) || !Number.isFinite(accuracyM) || accuracyM < 0) {
    return null;
  }
  return accuracyM;
}

function bearingDeg(lat1: number, lon1: number, lat2: number, lon2: number): number {
  const p1 = (lat1 * Math.PI) / 180;
  const p2 = (lat2 * Math.PI) / 180;
  const dLon = ((lon2 - lon1) * Math.PI) / 180;
  const y = Math.sin(dLon) * Math.cos(p2);
  const x = Math.cos(p1) * Math.sin(p2) - Math.sin(p1) * Math.cos(p2) * Math.cos(dLon);
  return (Math.atan2(y, x) * 180) / Math.PI;
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
