import { beforeEach, describe, expect, it } from "vitest";
import { LocalStoragePersistence } from "../integrations/persistence/LocalStoragePersistence.js";
import { LocationStore } from "../stores/LocationStore.js";
import type {
  LocationListener,
  LocationService,
  LocationUpdate,
} from "./location/BrowserLocationService.js";
import { formatSpeedLabel, mpsToUnit } from "./speed.js";

// Why existing tests didn't cover this: speed wasn't tracked anywhere — the
// old `LocationUpdate` had no `speedMps` and the LocationStore had no
// observable speed state. Spec line 31 ("speed is shown") was implemented
// only inside the routing card subtitle, never on its own badge.
class FakeLocationService implements LocationService {
  listener?: LocationListener;
  isSupported(): boolean {
    return true;
  }
  async permissionState(): Promise<"granted" | "prompt" | "denied" | "unknown"> {
    return "granted";
  }
  start(listener: LocationListener): () => void {
    this.listener = listener;
    return () => {
      this.listener = undefined;
    };
  }
  emit(update: LocationUpdate): void {
    this.listener?.(update);
  }
}

describe("speed badge formatting", () => {
  it("converts m/s to kph and rounds to whole units", () => {
    expect(mpsToUnit(5, "kph")).toBe(18);
    expect(mpsToUnit(0, "kph")).toBe(0);
  });

  it("converts m/s to mph and rounds to whole units", () => {
    expect(mpsToUnit(4.4704, "mph")).toBe(10);
  });

  it("formats with the unit label", () => {
    expect(formatSpeedLabel(5, "kph")).toBe("18 km/h");
    expect(formatSpeedLabel(4.4704, "mph")).toBe("10 mph");
  });

  it("renders a dash when speed is undefined / not yet known", () => {
    expect(formatSpeedLabel(undefined, "kph")).toBe("— km/h");
  });
});

describe("LocationStore speed tracking", () => {
  beforeEach(() => {
    localStorage.clear();
  });

  it("captures speedMps from each fix and exposes currentSpeedMps", () => {
    const service = new FakeLocationService();
    const store = new LocationStore(service, new LocalStoragePersistence());
    store.start();
    service.emit({ kind: "fix", point: { latitude: 60.2, longitude: 24.9 }, speedMps: 5.5 });
    expect(store.currentSpeedMps).toBeCloseTo(5.5, 5);
  });

  it("treats negative or NaN speed as unknown (undefined)", () => {
    const service = new FakeLocationService();
    const store = new LocationStore(service, new LocalStoragePersistence());
    store.start();
    service.emit({ kind: "fix", point: { latitude: 60.2, longitude: 24.9 }, speedMps: -1 });
    expect(store.currentSpeedMps).toBeUndefined();
  });
});
