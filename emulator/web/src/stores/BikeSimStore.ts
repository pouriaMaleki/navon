import { makeAutoObservable } from "mobx";
import {
  advanceBikePhysics,
  createInitialBikePhysicsState,
  DEFAULT_BIKE_PHYSICS_CONFIG,
  measureGroundSpeedKmh,
  toSimulatedGeoSample,
} from "../simulation/bikePhysics";
import type {
  BikeControlIntent,
  BikePhysicsConfig,
  BikePhysicsState,
  SimulatedGeoSample,
} from "../types";
import type { GeoStore } from "./GeoStore";

const DEFAULT_LAT = 60.17442;
const DEFAULT_LON = 24.9421;
const DEBUG_INTERVAL_MS = 1200;
const IDLE_DEBUG_INTERVAL_MS = 5000;
const MIN_WHEELBASE_M = 0.2;

export class BikeSimStore {
  readonly input: BikeControlIntent = {
    accelerate: false,
    brake: false,
    turnLeft: false,
    turnRight: false,
  };

  state: BikePhysicsState = createInitialBikePhysicsState(DEFAULT_LAT, DEFAULT_LON);
  readonly physicsConfig: BikePhysicsConfig = { ...DEFAULT_BIKE_PHYSICS_CONFIG };

  private rafId = 0;
  private lastFrameMs = 0;
  private lastDebugMs = 0;
  private lastIdleDebugMs = 0;
  private lastPublishDebugMs = 0;
  private running = false;
  private leftRequested = false;
  private rightRequested = false;
  private turnPriority: "left" | "right" | null = null;
  private speedMismatchSampleStreak = 0;

  constructor(private readonly geoStore: GeoStore) {
    makeAutoObservable<BikeSimStore, "geoStore" | "rafId" | "lastFrameMs" | "running">(
      this,
      {
        geoStore: false,
        rafId: false,
        lastFrameMs: false,
        running: false,
      },
      { autoBind: true },
    );
  }

  start(): void {
    if (this.running) {
      return;
    }
    this.running = true;
    this.lastFrameMs = performance.now();
    this.lastDebugMs = 0;
    this.lastIdleDebugMs = 0;
    this.lastPublishDebugMs = 0;
    document.addEventListener("keydown", this.onKeyDown, true);
    document.addEventListener("keyup", this.onKeyUp, true);
    window.addEventListener("blur", this.onWindowBlur);
    console.info("[emu:bike] start", {
      lat: this.state.lat,
      lon: this.state.lon,
    });
    this.rafId = window.requestAnimationFrame(this.onFrame);
  }

  reset(): void {
    this.state = createInitialBikePhysicsState(DEFAULT_LAT, DEFAULT_LON);
    this.releaseAllInputs();
    this.speedMismatchSampleStreak = 0;
    console.info("[emu:bike] reset", {
      lat: this.state.lat,
      lon: this.state.lon,
    });
    this.publishSample();
  }

  dispose(): void {
    this.running = false;
    window.cancelAnimationFrame(this.rafId);
    document.removeEventListener("keydown", this.onKeyDown, true);
    document.removeEventListener("keyup", this.onKeyUp, true);
    window.removeEventListener("blur", this.onWindowBlur);
    this.releaseAllInputs();
    this.speedMismatchSampleStreak = 0;
  }

  setAccelerate(active: boolean): void {
    if (this.input.accelerate === active) {
      return;
    }
    this.input.accelerate = active;
    console.info("[emu:bike] accelerate", { active });
  }

  setBrake(active: boolean): void {
    if (this.input.brake === active) {
      return;
    }
    this.input.brake = active;
    console.info("[emu:bike] brake", { active });
  }

  setTurnLeft(active: boolean): void {
    this.leftRequested = active;
    if (active) {
      this.turnPriority = "left";
    } else if (this.turnPriority === "left") {
      this.turnPriority = this.rightRequested ? "right" : null;
    }
    this.syncTurnState();
    console.info("[emu:bike] turn-left", {
      active,
      turnIntent: this.describeTurnIntent(),
      input: { turnLeft: this.input.turnLeft, turnRight: this.input.turnRight },
    });
  }

  setTurnRight(active: boolean): void {
    this.rightRequested = active;
    if (active) {
      this.turnPriority = "right";
    } else if (this.turnPriority === "right") {
      this.turnPriority = this.leftRequested ? "left" : null;
    }
    this.syncTurnState();
    console.info("[emu:bike] turn-right", {
      active,
      turnIntent: this.describeTurnIntent(),
      input: { turnLeft: this.input.turnLeft, turnRight: this.input.turnRight },
    });
  }

  get speedKmh(): number {
    return this.state.speedMps * 3.6;
  }

  get headingDeg(): number {
    return (this.state.headingRad * 180) / Math.PI;
  }

  setMaxSpeedKmh(value: number): void {
    this.physicsConfig.maxSpeedKmh = clampNumber(value, 5, 60);
  }

  setThrottleAccelMps2(value: number): void {
    this.physicsConfig.throttleAccelMps2 = clampNumber(value, 0.2, 6);
  }

  setCoastDecelMps2(value: number): void {
    this.physicsConfig.coastDecelMps2 = clampNumber(value, 0.05, 2.5);
  }

  setBrakeDecelMps2(value: number): void {
    this.physicsConfig.brakeDecelMps2 = clampNumber(value, 0.2, 6);
  }

  setMaxSteerDeg(value: number): void {
    this.physicsConfig.maxSteerDeg = clampNumber(value, 5, 35);
  }

  setSteerResponseDegPerSec(value: number): void {
    this.physicsConfig.steerResponseDegPerSec = clampNumber(value, 5, 90);
  }

  setWheelbaseM(value: number): void {
    this.physicsConfig.wheelbaseM = clampNumber(value, MIN_WHEELBASE_M, 2.5);
  }

  resetPhysicsConfig(): void {
    this.physicsConfig.maxSpeedKmh = DEFAULT_BIKE_PHYSICS_CONFIG.maxSpeedKmh;
    this.physicsConfig.throttleAccelMps2 = DEFAULT_BIKE_PHYSICS_CONFIG.throttleAccelMps2;
    this.physicsConfig.coastDecelMps2 = DEFAULT_BIKE_PHYSICS_CONFIG.coastDecelMps2;
    this.physicsConfig.brakeDecelMps2 = DEFAULT_BIKE_PHYSICS_CONFIG.brakeDecelMps2;
    this.physicsConfig.maxSteerDeg = DEFAULT_BIKE_PHYSICS_CONFIG.maxSteerDeg;
    this.physicsConfig.steerResponseDegPerSec = DEFAULT_BIKE_PHYSICS_CONFIG.steerResponseDegPerSec;
    this.physicsConfig.wheelbaseM = DEFAULT_BIKE_PHYSICS_CONFIG.wheelbaseM;
  }

  private readonly onFrame = (nowMs: number): void => {
    if (!this.running) {
      return;
    }
    const wallDtSec = Math.max(0, (nowMs - this.lastFrameMs) / 1000);
    this.lastFrameMs = nowMs;
    const prevState = this.state;
    const advanceResult = advanceBikePhysics(this.state, this.input, wallDtSec, this.physicsConfig);
    this.state = advanceResult.state;
    const hasActiveInput =
      this.input.accelerate || this.input.brake || this.input.turnLeft || this.input.turnRight;
    const isMoving = this.state.speedMps > 0.05;
    if (hasActiveInput || isMoving) {
      if (nowMs - this.lastDebugMs < DEBUG_INTERVAL_MS) {
        this.publishSample();
        this.rafId = window.requestAnimationFrame(this.onFrame);
        return;
      }
      this.lastDebugMs = nowMs;
      const headingDeltaDeg = toDegrees(
        signedAngleDelta(this.state.headingRad, prevState.headingRad),
      );
      const latDeltaMeters = toMetersLat(this.state.lat - prevState.lat);
      const lonDeltaMeters = toMetersLon(this.state.lon - prevState.lon, prevState.lat);
      const reportedSpeedKmh = this.state.speedMps * 3.6;
      const measuredSpeedKmh = measureGroundSpeedKmh(
        prevState.lat,
        prevState.lon,
        this.state.lat,
        this.state.lon,
        wallDtSec,
      );
      const speedErrorPct = calculateSpeedErrorPct(reportedSpeedKmh, measuredSpeedKmh);
      console.info("[emu:bike] frame", {
        input: { ...this.input },
        turnIntent: this.describeTurnIntent(),
        wallDtMs: Number((wallDtSec * 1000).toFixed(1)),
        simDtMs: Number((advanceResult.simDtSec * 1000).toFixed(1)),
        substeps: advanceResult.substeps,
        droppedDtMs: Number((advanceResult.droppedDtSec * 1000).toFixed(1)),
        reportedSpeedKmh: Number(reportedSpeedKmh.toFixed(2)),
        measuredSpeedKmh: Number(measuredSpeedKmh.toFixed(2)),
        speedErrorPct: Number(speedErrorPct.toFixed(1)),
        prevHeadingDeg: Number(toDegrees(prevState.headingRad).toFixed(1)),
        headingDeg: Number(((this.state.headingRad * 180) / Math.PI).toFixed(1)),
        headingDeltaDeg: Number(headingDeltaDeg.toFixed(2)),
        steerDeg: Number(((this.state.steerAngleRad * 180) / Math.PI).toFixed(1)),
        latDeltaM: Number(latDeltaMeters.toFixed(3)),
        lonDeltaM: Number(lonDeltaMeters.toFixed(3)),
        lat: Number(this.state.lat.toFixed(6)),
        lon: Number(this.state.lon.toFixed(6)),
      });
      this.evaluateSpeedMismatch(
        Math.abs(speedErrorPct),
        reportedSpeedKmh >= 5 || measuredSpeedKmh >= 5,
        {
          reportedSpeedKmh,
          measuredSpeedKmh,
          speedErrorPct,
          wallDtSec,
          simDtSec: advanceResult.simDtSec,
          droppedDtSec: advanceResult.droppedDtSec,
          substeps: advanceResult.substeps,
        },
      );
    } else if (nowMs - this.lastIdleDebugMs >= IDLE_DEBUG_INTERVAL_MS) {
      this.lastIdleDebugMs = nowMs;
      console.info("[emu:bike] idle", {
        speedKmh: Number((this.state.speedMps * 3.6).toFixed(2)),
        turnIntent: this.describeTurnIntent(),
        activeElement: describeElement(document.activeElement),
      });
    }
    this.publishSample();
    this.rafId = window.requestAnimationFrame(this.onFrame);
  };

  private publishSample(): void {
    const sample: SimulatedGeoSample = toSimulatedGeoSample(this.state);
    const nowMs = performance.now();
    const hasActiveInput =
      this.input.accelerate || this.input.brake || this.input.turnLeft || this.input.turnRight;
    const isMoving = this.state.speedMps > 0.05;
    if ((hasActiveInput || isMoving) && nowMs - this.lastPublishDebugMs >= DEBUG_INTERVAL_MS) {
      this.lastPublishDebugMs = nowMs;
      console.info("[emu:bike] publish-sample", {
        lat: Number(sample.lat.toFixed(6)),
        lon: Number(sample.lon.toFixed(6)),
        headingDeg: Number(((sample.headingRad * 180) / Math.PI).toFixed(1)),
        speedKmh: Number((sample.speedMps * 3.6).toFixed(2)),
      });
    }
    this.geoStore.applySimulatedSample(sample);
  }

  private readonly onKeyDown = (event: KeyboardEvent): void => {
    if (!isControlKey(event.key)) {
      return;
    }
    if (isEditableTarget(event.target)) {
      console.info("[emu:bike] key-ignored-editable", {
        key: event.key,
        target: describeElement(event.target),
      });
      return;
    }
    event.preventDefault();
    if (event.repeat) {
      console.info("[emu:bike] key-repeat-ignored", event.key);
      return;
    }
    this.setInputForKey(event.key, true);
    console.info("[emu:bike] key-down", event.key);
  };

  private readonly onKeyUp = (event: KeyboardEvent): void => {
    if (!isControlKey(event.key)) {
      return;
    }
    event.preventDefault();
    this.setInputForKey(event.key, false);
    console.info("[emu:bike] key-up", event.key);
  };

  private setInputForKey(key: string, active: boolean): void {
    switch (key) {
      case "ArrowUp":
        this.setAccelerate(active);
        break;
      case "ArrowDown":
        this.setBrake(active);
        break;
      case "ArrowLeft":
        this.setTurnLeft(active);
        break;
      case "ArrowRight":
        this.setTurnRight(active);
        break;
      default:
        break;
    }
  }

  private releaseAllInputs(): void {
    this.input.accelerate = false;
    this.input.brake = false;
    this.leftRequested = false;
    this.rightRequested = false;
    this.turnPriority = null;
    this.syncTurnState();
  }

  private syncTurnState(): void {
    if (this.leftRequested && this.rightRequested) {
      this.input.turnLeft = this.turnPriority === "left";
      this.input.turnRight = this.turnPriority === "right";
      return;
    }
    this.input.turnLeft = this.leftRequested;
    this.input.turnRight = this.rightRequested;
  }

  private describeTurnIntent(): string {
    if (this.leftRequested && this.rightRequested) {
      return this.turnPriority === "left" ? "both(left-wins)" : "both(right-wins)";
    }
    if (this.leftRequested) {
      return "left";
    }
    if (this.rightRequested) {
      return "right";
    }
    return "neutral";
  }

  private evaluateSpeedMismatch(
    absSpeedErrorPct: number,
    mismatchEligible: boolean,
    context: {
      reportedSpeedKmh: number;
      measuredSpeedKmh: number;
      speedErrorPct: number;
      wallDtSec: number;
      simDtSec: number;
      droppedDtSec: number;
      substeps: number;
    },
  ): void {
    if (mismatchEligible && absSpeedErrorPct > 15) {
      this.speedMismatchSampleStreak += 1;
      if (this.speedMismatchSampleStreak >= 3) {
        console.warn("[emu:bike] speed-mismatch", {
          streak: this.speedMismatchSampleStreak,
          reportedSpeedKmh: Number(context.reportedSpeedKmh.toFixed(2)),
          measuredSpeedKmh: Number(context.measuredSpeedKmh.toFixed(2)),
          speedErrorPct: Number(context.speedErrorPct.toFixed(1)),
          wallDtMs: Number((context.wallDtSec * 1000).toFixed(1)),
          simDtMs: Number((context.simDtSec * 1000).toFixed(1)),
          droppedDtMs: Number((context.droppedDtSec * 1000).toFixed(1)),
          substeps: context.substeps,
        });
      }
      return;
    }
    this.speedMismatchSampleStreak = 0;
  }

  private readonly onWindowBlur = (): void => {
    this.releaseAllInputs();
    console.info("[emu:bike] window blur -> released controls");
  };
}

function isControlKey(key: string): boolean {
  return key === "ArrowUp" || key === "ArrowDown" || key === "ArrowLeft" || key === "ArrowRight";
}

function isEditableTarget(target: EventTarget | null): boolean {
  return (
    target instanceof HTMLInputElement ||
    target instanceof HTMLTextAreaElement ||
    target instanceof HTMLSelectElement ||
    (target instanceof HTMLElement && target.isContentEditable)
  );
}

function describeElement(target: EventTarget | null): string {
  if (!(target instanceof Element)) {
    return "unknown";
  }
  const tag = target.tagName.toLowerCase();
  const id = target.id ? `#${target.id}` : "";
  const className =
    typeof target.className === "string" && target.className.trim().length > 0
      ? `.${target.className.trim().split(/\s+/).join(".")}`
      : "";
  return `${tag}${id}${className}`;
}

function toDegrees(radians: number): number {
  return (radians * 180) / Math.PI;
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

function toMetersLat(deltaLatDeg: number): number {
  return deltaLatDeg * 111_320;
}

function toMetersLon(deltaLonDeg: number, latDeg: number): number {
  return deltaLonDeg * 111_320 * Math.max(0.00001, Math.cos((latDeg * Math.PI) / 180));
}

function calculateSpeedErrorPct(reportedSpeedKmh: number, measuredSpeedKmh: number): number {
  const baselineKmh = Math.max(0.1, reportedSpeedKmh);
  return ((measuredSpeedKmh - reportedSpeedKmh) / baselineKmh) * 100;
}

function clampNumber(value: number, min: number, max: number): number {
  if (!Number.isFinite(value)) {
    return min;
  }
  return Math.max(min, Math.min(max, value));
}
