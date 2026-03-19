import { WAVESHARE_ESP32_P4_3_4 } from "../core/screenProfiles";
import type { ScreenProfile } from "../core/types";
import { BikeSimStore } from "./BikeSimStore";
import { EmulatorStore } from "./EmulatorStore";
import { GeoStore } from "./GeoStore";
import { TouchStore } from "./TouchStore";

type ScreenProfileFactory = (canvas: HTMLCanvasElement) => ScreenProfile;

type AppStoreOptions = {
  screenProfile?: ScreenProfile | ScreenProfileFactory;
  autoRequestLiveGps?: boolean;
};

export class AppStore {
  readonly bikeSimStore: BikeSimStore;
  readonly geoStore: GeoStore;
  readonly touchStore: TouchStore;
  readonly emulatorStore: EmulatorStore;

  constructor(options?: AppStoreOptions) {
    const configuredProfile = options?.screenProfile;
    const profileFactory =
      typeof configuredProfile === "function"
        ? configuredProfile
        : () => configuredProfile ?? WAVESHARE_ESP32_P4_3_4;
    this.geoStore =
      options?.autoRequestLiveGps === undefined
        ? new GeoStore()
        : new GeoStore({ autoRequestLiveGps: options.autoRequestLiveGps });
    this.bikeSimStore = new BikeSimStore(this.geoStore);
    this.touchStore = new TouchStore();
    this.emulatorStore = new EmulatorStore(this, profileFactory);
  }

  dispose(): void {
    this.emulatorStore.dispose();
  }
}
