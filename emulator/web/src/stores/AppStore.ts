import { BikeSimStore } from "./BikeSimStore";
import { EmulatorStore } from "./EmulatorStore";
import { GeoStore } from "./GeoStore";
import { TouchStore } from "./TouchStore";

export class AppStore {
  readonly bikeSimStore: BikeSimStore;
  readonly geoStore: GeoStore;
  readonly touchStore: TouchStore;
  readonly emulatorStore: EmulatorStore;

  constructor() {
    this.geoStore = new GeoStore();
    this.bikeSimStore = new BikeSimStore(this.geoStore);
    this.touchStore = new TouchStore();
    this.emulatorStore = new EmulatorStore(this);
  }

  dispose(): void {
    this.emulatorStore.dispose();
  }
}
