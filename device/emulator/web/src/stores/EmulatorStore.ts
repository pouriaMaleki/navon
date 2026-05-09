import { makeAutoObservable, runInAction } from "mobx";
import { Esp32ScreenEmulator } from "../core/emulator";
import type { ScreenProfile } from "../core/types";
import { createWasmProgram, importGpxRouteSyncFromFile } from "../programs/wasmProgram";
import type { RuntimeRouteSyncInput, WasmRuntimeState } from "../types";
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
  importedRouteFileName: string | null = null;
  routeImportStatus = "Using built-in demo route";
  isImportingRoute = false;

  private importedRouteSync: RuntimeRouteSyncInput | null = null;
  private emulator: Esp32ScreenEmulator<WasmRuntimeState> | null = null;
  private customState: WasmRuntimeState | null = null;
  private activeProfile: ScreenProfile | null = null;
  private activeCanvas: HTMLCanvasElement | null = null;

  constructor(
    private readonly appStore: AppStore,
    private readonly profileFactory: ScreenProfileFactory,
  ) {
    makeAutoObservable<EmulatorStore, "appStore" | "emulator" | "customState" | "activeCanvas">(
      this,
      {
        appStore: false,
        emulator: false,
        customState: false,
        activeCanvas: false,
      },
      { autoBind: true },
    );
  }

  async init(canvas: HTMLCanvasElement): Promise<void> {
    this.activeCanvas = canvas;
    await this.startForCanvas(canvas);
  }

  async syncCanvasProfile(canvas: HTMLCanvasElement): Promise<void> {
    this.activeCanvas = canvas;
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

  async restartRuntime(): Promise<void> {
    if (!this.activeCanvas) {
      return;
    }
    await this.startForCanvas(this.activeCanvas);
  }

  private async startForCanvas(
    canvas: HTMLCanvasElement,
    profileOverride?: ScreenProfile,
  ): Promise<void> {
    this.activeCanvas = canvas;
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
      const { initialState, program } = await createWasmProgram(0, {
        routeAlertVerbosity: this.appStore.routeAlertVerbosity,
      });
      const profile = profileOverride ?? this.profileFactory(canvas);
      this.emulator = new Esp32ScreenEmulator(canvas, profile, initialState, program, {
        onFrame: this.recordFrameTiming,
      });
      this.activeProfile = profile;
      this.customState = this.emulator.customState();
      if (this.importedRouteSync && this.customState) {
        this.customState.queuedRouteSync = this.importedRouteSync;
        this.customState.routeSeeded = true;
      }
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

  async importGpxFile(file: Pick<File, "text" | "name">): Promise<void> {
    this.isImportingRoute = true;
    this.errorMessage = null;
    try {
      const routeSync = await importGpxRouteSyncFromFile(file);
      this.importedRouteSync = routeSync;
      this.importedRouteFileName = file.name;
      this.routeImportStatus = `Loaded ${file.name}`;
      if (this.customState) {
        this.customState.queuedRouteSync = routeSync;
        this.customState.routeSeeded = true;
      }
    } catch (error) {
      this.routeImportStatus = error instanceof Error ? error.message : "Failed to import GPX route";
      this.errorMessage = this.routeImportStatus;
    } finally {
      this.isImportingRoute = false;
    }
  }

  clearImportedRoute(): void {
    this.importedRouteSync = null;
    this.importedRouteFileName = null;
    this.routeImportStatus = "Using built-in demo route";
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
    this.activeCanvas = null;
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
