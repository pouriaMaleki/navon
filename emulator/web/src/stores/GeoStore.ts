import { makeAutoObservable } from "mobx";
import type { RuntimeGpsInput, SimulatedGeoSample, WasmRuntimeState } from "../types";

const DEFAULT_LAT = 60.17442;
const DEFAULT_LON = 24.9421;
const EARTH_RADIUS_M = 6_371_000;
const MOTION_DISTANCE_NOISE_M = 1.5;

export class GeoStore {
  statusText = "GPS: initializing";
  isLive = false;

  private watchId: number | null = null;
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
      "watchId" | "customState" | "prevLat" | "prevLon" | "hasPrev" | "prevTsMs" | "simulatedSample"
    >(
      this,
      {
        watchId: false,
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
    this.enableSimulatedMode();
    this.requestLiveGps();
  }

  requestLiveGps(): void {
    if (!this.customState) {
      return;
    }

    if (!("geolocation" in navigator)) {
      this.statusText = "GPS: unavailable (bike sim)";
      return;
    }

    if (!window.isSecureContext) {
      this.statusText = "GPS: blocked (secure context required, bike sim)";
      return;
    }

    if (this.watchId !== null) {
      navigator.geolocation.clearWatch(this.watchId);
      this.watchId = null;
    }

    this.statusText = "GPS: requesting permission";
    this.watchId = navigator.geolocation.watchPosition(
      (position) => {
        if (!this.customState) {
          return;
        }
        const lat = position.coords.latitude;
        const lon = position.coords.longitude;
        const tsMs = position.timestamp;
        this.isLive = true;
        this.statusText = "GPS: live";

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
      },
      (error) => {
        if (!this.customState) {
          return;
        }
        this.enableSimulatedMode();
        if (error.code === error.PERMISSION_DENIED) {
          this.statusText = "GPS: denied (bike sim)";
          return;
        }
        this.statusText = "GPS: error (bike sim)";
      },
      { enableHighAccuracy: true, maximumAge: 1000, timeout: 10000 },
    );
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
    this.statusText = "GPS: simulated (bike controls)";
  }

  dispose(): void {
    if (this.watchId !== null && "geolocation" in navigator) {
      navigator.geolocation.clearWatch(this.watchId);
      this.watchId = null;
    }
    this.customState = null;
    this.hasPrev = false;
    this.prevTsMs = 0;
  }

  private enableSimulatedMode(): void {
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
    this.statusText = "GPS: simulated (bike controls)";
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
