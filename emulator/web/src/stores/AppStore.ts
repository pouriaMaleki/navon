import { EmulatorStore } from "./EmulatorStore";
import { GeoStore } from "./GeoStore";
import { TouchStore } from "./TouchStore";

export class AppStore {
  readonly geoStore: GeoStore;
  readonly touchStore: TouchStore;
  readonly emulatorStore: EmulatorStore;

  constructor() {
    this.geoStore = new GeoStore();
    this.touchStore = new TouchStore();
    this.emulatorStore = new EmulatorStore(this);
  }

  dispose(): void {
    this.emulatorStore.dispose();
  }
}
