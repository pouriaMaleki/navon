import { beforeEach, describe, expect, it, vi } from "vitest";
import type {
  LocationListener,
  LocationService,
  LocationUpdate,
} from "../integrations/location/BrowserLocationService.js";
import { LocalStoragePersistence } from "../integrations/persistence/LocalStoragePersistence.js";
import { LocationStore } from "../stores/LocationStore.js";

class FakeLocationService implements LocationService {
  listener?: LocationListener;
  startCount = 0;
  stopCount = 0;
  permissionResult: "granted" | "prompt" | "denied" | "unknown" = "prompt";
  supported = true;

  isSupported(): boolean {
    return this.supported;
  }
  async permissionState(): Promise<"granted" | "prompt" | "denied" | "unknown"> {
    return this.permissionResult;
  }
  start(listener: LocationListener): () => void {
    this.startCount += 1;
    this.listener = listener;
    return () => {
      this.stopCount += 1;
      this.listener = undefined;
    };
  }
  emit(update: LocationUpdate): void {
    this.listener?.(update);
  }
}

describe("LocationStore", () => {
  beforeEach(() => {
    localStorage.clear();
  });

  it("starts in idle state with no fix", () => {
    const store = new LocationStore(new FakeLocationService(), new LocalStoragePersistence());
    expect(store.isLocating).toBe(false);
    expect(store.currentLocation).toBeNull();
    expect(store.bestKnownLocation()).toBeNull();
    expect(store.isWaitingForFirstFix).toBe(false);
  });

  it("transitions idle → locating → fixed and persists the fix", () => {
    const service = new FakeLocationService();
    const persistence = new LocalStoragePersistence();
    const store = new LocationStore(service, persistence);

    store.start();
    expect(store.isLocating).toBe(true);
    expect(store.isWaitingForFirstFix).toBe(true);

    service.emit({ kind: "fix", point: { latitude: 60.2, longitude: 24.9 } });

    expect(store.isLocating).toBe(false);
    expect(store.currentLocation).toEqual({ latitude: 60.2, longitude: 24.9 });
    expect(store.permission).toBe("granted");
    expect(persistence.loadLastKnownRider()).toEqual({ latitude: 60.2, longitude: 24.9 });
  });

  it("flips to denied state on permission error", () => {
    const service = new FakeLocationService();
    const store = new LocationStore(service, new LocalStoragePersistence());
    store.start();
    service.emit({ kind: "error", error: "denied", message: "User denied geolocation" });
    expect(store.isLocating).toBe(false);
    expect(store.lastError).toBe("denied");
    expect(store.permission).toBe("denied");
    expect(store.currentLocation).toBeNull();
  });

  it("falls back to last-known location from persistence on cold boot", () => {
    const persistence = new LocalStoragePersistence();
    persistence.saveLastKnownRider({ latitude: 51.5, longitude: -0.1 });
    const store = new LocationStore(new FakeLocationService(), persistence);
    expect(store.bestKnownLocation()).toEqual({ latitude: 51.5, longitude: -0.1 });
    expect(store.isWaitingForFirstFix).toBe(false);
  });

  it("is no-op when geolocation is unsupported", () => {
    const service = new FakeLocationService();
    service.supported = false;
    const store = new LocationStore(service, new LocalStoragePersistence());
    store.start();
    expect(store.lastError).toBe("unsupported");
    expect(service.startCount).toBe(0);
  });

  it("stop() releases the watcher and is idempotent", () => {
    const service = new FakeLocationService();
    const store = new LocationStore(service, new LocalStoragePersistence());
    store.start();
    store.stop();
    store.stop();
    expect(service.stopCount).toBe(1);
  });

  it("ignores re-entrant start() calls while already watching", () => {
    const service = new FakeLocationService();
    const store = new LocationStore(service, new LocalStoragePersistence());
    store.start();
    store.start();
    expect(service.startCount).toBe(1);
  });

  it("permission refresh updates state from the service", async () => {
    const service = new FakeLocationService();
    service.permissionResult = "granted";
    const store = new LocationStore(service, new LocalStoragePersistence());
    await vi.waitFor(() => {
      expect(store.permission).toBe("granted");
    });
  });
});
