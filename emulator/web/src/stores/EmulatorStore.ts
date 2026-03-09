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
      );
      this.customState = this.emulator.customState();
      this.emulator.renderOnce();
      this.emulator.start();

      this.appStore.touchStore.bind(canvas, this.customState);
      this.appStore.geoStore.bind(this.customState);
      this.appStore.geoStore.start();

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
    this.emulator.reset();
  }

  dispose(): void {
    this.appStore.geoStore.dispose();
    this.appStore.touchStore.dispose();
    if (this.emulator) {
      this.emulator.stop();
    }
    this.emulator = null;
    this.customState = null;
    this.isReady = false;
    this.isRunning = false;
    this.isLoading = false;
  }
}
