import { makeAutoObservable, runInAction } from "mobx";
import { Esp32ScreenEmulator } from "../core/emulator";
import { WAVESHARE_ESP32_P4_3_4 } from "../core/screenProfiles";
import { createWasmProgram } from "../programs/wasmProgram";
import type { WasmRuntimeState } from "../types";
import type { AppStore } from "./AppStore";

export class EmulatorStore {
  isReady = false;
  isRunning = false;
  isLoading = false;
  errorMessage: string | null = null;
  frameDtMs = 0;
  frameDtAvgMs = 0;
  fps = 0;
  frameSamples = 0;

  private emulator: Esp32ScreenEmulator<WasmRuntimeState> | null = null;
  private customState: WasmRuntimeState | null = null;

  constructor(private readonly appStore: AppStore) {
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
    if (this.emulator || this.isLoading) {
      return;
    }
    this.isLoading = true;
    this.errorMessage = null;

    try {
      const { initialState, program } = await createWasmProgram(0);
      this.emulator = new Esp32ScreenEmulator(
        canvas,
        WAVESHARE_ESP32_P4_3_4,
        initialState,
        program,
        { onFrame: this.recordFrameTiming },
      );
      this.customState = this.emulator.customState();
      this.emulator.renderOnce();
      this.emulator.start();

      this.appStore.touchStore.bind(canvas, this.customState);
      this.appStore.geoStore.bind(this.customState);
      this.appStore.geoStore.start();
      this.appStore.bikeSimStore.start();
      this.appStore.bikeSimStore.reset();

      runInAction(() => {
        this.isRunning = true;
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

  toggleRunning(): void {
    if (!this.emulator) {
      return;
    }
    this.isRunning = this.emulator.toggle();
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
    this.appStore.geoStore.dispose();
    this.appStore.bikeSimStore.dispose();
    this.appStore.touchStore.dispose();
    if (this.emulator) {
      this.emulator.stop();
    }
    this.emulator = null;
    this.customState = null;
    this.resetFrameTiming();
    this.isReady = false;
    this.isRunning = false;
    this.isLoading = false;
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
