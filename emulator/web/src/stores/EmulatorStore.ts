import { makeAutoObservable, runInAction } from "mobx";
import { Esp32ScreenEmulator } from "../core/emulator";
import type { ScreenProfile } from "../core/types";
import { createWasmProgram } from "../programs/wasmProgram";
import type { WasmRuntimeState } from "../types";
import type { AppStore } from "./AppStore";

type ScreenProfileFactory = (canvas: HTMLCanvasElement) => ScreenProfile;

export class EmulatorStore {
  isReady = false;
  isLoading = false;
  errorMessage: string | null = null;
  frameDtMs = 0;
  frameDtAvgMs = 0;
  fps = 0;
  frameSamples = 0;

  private emulator: Esp32ScreenEmulator<WasmRuntimeState> | null = null;
  private customState: WasmRuntimeState | null = null;
  private activeProfile: ScreenProfile | null = null;

  constructor(
    private readonly appStore: AppStore,
    private readonly profileFactory: ScreenProfileFactory,
  ) {
    makeAutoObservable<EmulatorStore, "appStore" | "emulator" | "customState">(
      this,
      {
        appStore: false,
        emulator: false,
        customState: false,
      },
      { autoBind: true },
    );
  }

  async init(canvas: HTMLCanvasElement): Promise<void> {
    await this.startForCanvas(canvas);
  }

  async syncCanvasProfile(canvas: HTMLCanvasElement): Promise<void> {
    const nextProfile = this.profileFactory(canvas);
    if (!this.emulator) {
      await this.startForCanvas(canvas, nextProfile);
      return;
    }
    if (
      this.activeProfile &&
      this.activeProfile.width === nextProfile.width &&
      this.activeProfile.height === nextProfile.height
    ) {
      return;
    }

    await this.startForCanvas(canvas, nextProfile);
  }

  private async startForCanvas(
    canvas: HTMLCanvasElement,
    profileOverride?: ScreenProfile,
  ): Promise<void> {
    if (this.emulator || this.isLoading) {
      if (
        this.isLoading ||
        (this.activeProfile &&
          profileOverride &&
          this.activeProfile.width === profileOverride.width &&
          this.activeProfile.height === profileOverride.height)
      ) {
        return;
      }
    }
    this.isLoading = true;
    this.errorMessage = null;
    this.shutdownRuntime();

    try {
      const { initialState, program } = await createWasmProgram(0);
      const profile = profileOverride ?? this.profileFactory(canvas);
      this.emulator = new Esp32ScreenEmulator(canvas, profile, initialState, program, {
        onFrame: this.recordFrameTiming,
      });
      this.activeProfile = profile;
      this.customState = this.emulator.customState();
      this.emulator.renderOnce();
      this.emulator.start();

      this.appStore.touchStore.bind(canvas, this.customState);
      this.appStore.geoStore.bind(this.customState);
      this.appStore.geoStore.start();
      this.appStore.bikeSimStore.start();
      this.appStore.bikeSimStore.reset();

      runInAction(() => {
        this.isReady = true;
      });
    } catch (error) {
      runInAction(() => {
        this.errorMessage = error instanceof Error ? error.message : "Failed to start emulator";
      });
    } finally {
      runInAction(() => {
        this.isLoading = false;
      });
    }
  }

  reset(): void {
    if (!this.emulator) {
      return;
    }
    this.appStore.bikeSimStore.reset();
    this.resetFrameTiming();
    this.emulator.reset();
  }

  dispose(): void {
    this.shutdownRuntime();
    this.resetFrameTiming();
    this.activeProfile = null;
    this.isReady = false;
    this.isLoading = false;
  }

  private shutdownRuntime(): void {
    this.appStore.geoStore.dispose();
    this.appStore.bikeSimStore.dispose();
    this.appStore.touchStore.dispose();
    if (this.emulator) {
      this.emulator.stop();
    }
    this.emulator = null;
    this.customState = null;
    this.activeProfile = null;
  }

  private recordFrameTiming(dtMs: number): void {
    this.frameDtMs = dtMs;
    this.frameSamples = Math.min(this.frameSamples + 1, 120);
    const blend = this.frameSamples === 1 ? 1 : 0.12;
    this.frameDtAvgMs =
      this.frameSamples === 1 ? dtMs : this.frameDtAvgMs + (dtMs - this.frameDtAvgMs) * blend;
    this.fps = this.frameDtAvgMs > 0 ? 1000 / this.frameDtAvgMs : 0;
  }

  private resetFrameTiming(): void {
    this.frameDtMs = 0;
    this.frameDtAvgMs = 0;
    this.fps = 0;
    this.frameSamples = 0;
  }
}
