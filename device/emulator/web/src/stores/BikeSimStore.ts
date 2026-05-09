import { makeAutoObservable } from "mobx";
import {
  advanceBikePhysics,
  createInitialBikePhysicsState,
  DEFAULT_BIKE_PHYSICS_CONFIG,
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
  private running = false;
  private leftRequested = false;
  private rightRequested = false;
  private turnPriority: "left" | "right" | null = null;

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
    document.addEventListener("keydown", this.onKeyDown, true);
    document.addEventListener("keyup", this.onKeyUp, true);
    window.addEventListener("blur", this.onWindowBlur);
    this.rafId = window.requestAnimationFrame(this.onFrame);
  }

  reset(): void {
    this.state = createInitialBikePhysicsState(DEFAULT_LAT, DEFAULT_LON);
    this.releaseAllInputs();
    this.publishSample();
  }

  dispose(): void {
    this.running = false;
    window.cancelAnimationFrame(this.rafId);
    document.removeEventListener("keydown", this.onKeyDown, true);
    document.removeEventListener("keyup", this.onKeyUp, true);
    window.removeEventListener("blur", this.onWindowBlur);
    this.releaseAllInputs();
  }

  setAccelerate(active: boolean): void {
    if (this.input.accelerate === active) {
      return;
    }
    this.input.accelerate = active;
  }

  setBrake(active: boolean): void {
    if (this.input.brake === active) {
      return;
    }
    this.input.brake = active;
  }

  setTurnLeft(active: boolean): void {
    this.leftRequested = active;
    if (active) {
      this.turnPriority = "left";
    } else if (this.turnPriority === "left") {
      this.turnPriority = this.rightRequested ? "right" : null;
    }
    this.syncTurnState();
  }

  setTurnRight(active: boolean): void {
    this.rightRequested = active;
    if (active) {
      this.turnPriority = "right";
    } else if (this.turnPriority === "right") {
      this.turnPriority = this.leftRequested ? "left" : null;
    }
    this.syncTurnState();
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
    this.state = advanceBikePhysics(this.state, this.input, wallDtSec, this.physicsConfig).state;
    this.publishSample();
    this.rafId = window.requestAnimationFrame(this.onFrame);
  };

  private publishSample(): void {
    const sample: SimulatedGeoSample = toSimulatedGeoSample(this.state);
    this.geoStore.applySimulatedSample(sample);
  }

  private readonly onKeyDown = (event: KeyboardEvent): void => {
    if (!isControlKey(event.key)) {
      return;
    }
    if (isEditableTarget(event.target)) {
      return;
    }
    event.preventDefault();
    if (event.repeat) {
      return;
    }
    this.setInputForKey(event.key, true);
  };

  private readonly onKeyUp = (event: KeyboardEvent): void => {
    if (!isControlKey(event.key)) {
      return;
    }
    event.preventDefault();
    this.setInputForKey(event.key, false);
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

  private readonly onWindowBlur = (): void => {
    this.releaseAllInputs();
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

function clampNumber(value: number, min: number, max: number): number {
  if (!Number.isFinite(value)) {
    return min;
  }
  return Math.max(min, Math.min(max, value));
}
