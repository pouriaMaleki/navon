import { makeAutoObservable } from "mobx";
import type { GeoCoordinates, SimulatedGeoSample, WasmRuntimeState } from "../types";

const DEFAULT_LAT = 60.17442;
const DEFAULT_LON = 24.9421;
const EARTH_RADIUS_M = 6_371_000;
const MOTION_DISTANCE_NOISE_M = 1.5;
const DEBUG_INTERVAL_MS = 500;

export class GeoStore {
  statusText = "GPS: initializing";
  isLive = false;

  private watchId: number | null = null;
  private customState: WasmRuntimeState | null = null;
  private prevLat = 0;
  private prevLon = 0;
  private hasPrev = false;
  private prevTsMs = 0;
  private lastDebugMs = 0;
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
        const coords: GeoCoordinates = { lat, lon };
        this.customState.hasGeo = true;
        this.customState.lat = coords.lat;
        this.customState.lon = coords.lon;
        this.isLive = true;
        this.statusText = "GPS: live";

        let distM = 0;
        if (this.hasPrev) {
          distM = haversineMeters(this.prevLat, this.prevLon, lat, lon);
          const dtS = Math.max(0.001, (tsMs - this.prevTsMs) / 1000);
          if (distM <= MOTION_DISTANCE_NOISE_M) {
            this.customState.speedMps = 0;
          } else {
            this.customState.speedMps = distM / dtS;
          }
        } else {
          this.customState.speedMps = 0;
        }

        let heading = position.coords.heading;
        if (
          (heading === null || Number.isNaN(heading)) &&
          this.hasPrev &&
          distM > MOTION_DISTANCE_NOISE_M
        ) {
          heading = bearingDeg(this.prevLat, this.prevLon, lat, lon);
        }
        if (heading !== null && !Number.isNaN(heading)) {
          this.customState.headingRad = (heading * Math.PI) / 180;
        }
        this.simulatedSample = {
          lat,
          lon,
          headingRad: this.customState.headingRad,
          speedMps: this.customState.speedMps,
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
      if (this.isLive) {
        console.debug("[emu:geo] ignored simulated sample because live GPS is active");
      }
      return;
    }
    this.customState.hasGeo = true;
    this.customState.lat = sample.lat;
    this.customState.lon = sample.lon;
    this.customState.headingRad = sample.headingRad;
    this.customState.speedMps = sample.speedMps;
    this.statusText = "GPS: simulated (bike controls)";
    const nowMs = performance.now();
    if (nowMs - this.lastDebugMs >= DEBUG_INTERVAL_MS) {
      this.lastDebugMs = nowMs;
      console.debug("[emu:geo] applied simulated sample", {
        lat: Number(sample.lat.toFixed(6)),
        lon: Number(sample.lon.toFixed(6)),
        headingDeg: Number(((sample.headingRad * 180) / Math.PI).toFixed(1)),
        speedKmh: Number((sample.speedMps * 3.6).toFixed(2)),
        hasGeo: this.customState.hasGeo,
      });
    }
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
    this.customState.hasGeo = true;
    this.customState.lat = this.simulatedSample.lat;
    this.customState.lon = this.simulatedSample.lon;
    this.customState.headingRad = this.simulatedSample.headingRad;
    this.customState.speedMps = this.simulatedSample.speedMps;
    this.statusText = "GPS: simulated (bike controls)";
    console.debug("[emu:geo] enabled simulated mode", {
      lat: Number(this.simulatedSample.lat.toFixed(6)),
      lon: Number(this.simulatedSample.lon.toFixed(6)),
    });
  }
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
