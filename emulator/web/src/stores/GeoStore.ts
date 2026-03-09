import { makeAutoObservable } from "mobx";
import type { GeoCoordinates, WasmRuntimeState } from "../types";

const DEFAULT_LAT = 60.17442;
const DEFAULT_LON = 24.9421;
const EARTH_RADIUS_M = 6_371_000;

export class GeoStore {
  statusText = "GPS: initializing";
  isLive = false;

  private watchId: number | null = null;
  private simTimerId: number | null = null;
  private customState: WasmRuntimeState | null = null;
  private prevLat = 0;
  private prevLon = 0;
  private hasPrev = false;
  private prevTsMs = 0;

  constructor() {
    makeAutoObservable<
      GeoStore,
      "watchId" | "simTimerId" | "customState" | "prevLat" | "prevLon" | "hasPrev" | "prevTsMs"
    >(
      this,
      {
        watchId: false,
        simTimerId: false,
        customState: false,
        prevLat: false,
        prevLon: false,
        hasPrev: false,
        prevTsMs: false,
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
    this.startSimulation();
    this.requestLiveGps();
  }

  requestLiveGps(): void {
    if (!this.customState) {
      return;
    }

    if (!("geolocation" in navigator)) {
      this.statusText = "GPS: unavailable (simulated)";
      return;
    }

    if (!window.isSecureContext) {
      this.statusText = "GPS: blocked (secure context required, simulated)";
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

        let heading = position.coords.heading;
        if ((heading === null || Number.isNaN(heading)) && this.hasPrev) {
          heading = bearingDeg(this.prevLat, this.prevLon, lat, lon);
        }
        if (heading !== null && !Number.isNaN(heading)) {
          this.customState.headingRad = (heading * Math.PI) / 180;
        }

        if (this.hasPrev) {
          const dtS = Math.max(0.001, (tsMs - this.prevTsMs) / 1000);
          const distM = haversineMeters(this.prevLat, this.prevLon, lat, lon);
          this.customState.speedMps = distM / dtS;
        } else {
          this.customState.speedMps = 0;
        }

        this.prevLat = lat;
        this.prevLon = lon;
        this.prevTsMs = tsMs;
        this.hasPrev = true;
      },
      (error) => {
        if (!this.customState) {
          return;
        }
        this.customState.hasGeo = false;
        this.customState.speedMps = 0;
        this.isLive = false;
        if (error.code === error.PERMISSION_DENIED) {
          this.statusText = "GPS: denied (simulated)";
          return;
        }
        this.statusText = "GPS: error (simulated)";
      },
      { enableHighAccuracy: true, maximumAge: 1000, timeout: 10000 },
    );
  }

  dispose(): void {
    if (this.watchId !== null && "geolocation" in navigator) {
      navigator.geolocation.clearWatch(this.watchId);
      this.watchId = null;
    }
    if (this.simTimerId !== null) {
      window.clearInterval(this.simTimerId);
      this.simTimerId = null;
    }
    this.customState = null;
    this.hasPrev = false;
    this.prevTsMs = 0;
  }

  private startSimulation(): void {
    if (!this.customState) {
      return;
    }
    this.statusText = "GPS: requesting permission";
    this.isLive = false;
    this.customState.hasGeo = false;
    this.customState.lat = DEFAULT_LAT;
    this.customState.lon = DEFAULT_LON;
    this.customState.headingRad = 0;
    this.customState.speedMps = 0;

    let t = 0;
    if (this.simTimerId !== null) {
      window.clearInterval(this.simTimerId);
    }
    this.simTimerId = window.setInterval(() => {
      if (!this.customState || this.customState.hasGeo) {
        return;
      }
      t += 0.05;
      this.customState.lat = DEFAULT_LAT + Math.sin(t) * 0.0012;
      this.customState.lon = DEFAULT_LON + Math.cos(t) * 0.0012;
      this.customState.headingRad = t + Math.PI / 2;
      this.customState.speedMps = 3.8;
    }, 100);
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
