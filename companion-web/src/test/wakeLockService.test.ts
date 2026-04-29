import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { WakeLockService } from "../integrations/screen/WakeLockService.js";

type FakeSentinel = {
  released: boolean;
  release: () => Promise<void>;
};

function installFakeWakeLock() {
  const sentinels: FakeSentinel[] = [];
  let lastSentinel: FakeSentinel | undefined;
  const fake = {
    request: vi.fn(async (_type: string) => {
      const sentinel: FakeSentinel = {
        released: false,
        release: vi.fn(async () => {
          sentinel.released = true;
        }),
      };
      sentinels.push(sentinel);
      lastSentinel = sentinel;
      return sentinel;
    }),
  };
  Object.defineProperty(navigator, "wakeLock", {
    configurable: true,
    value: fake,
  });
  return {
    fake,
    get sentinels() {
      return sentinels;
    },
    get last() {
      return lastSentinel;
    },
  };
}

function fireVisibilityChange(state: "visible" | "hidden") {
  Object.defineProperty(document, "visibilityState", {
    configurable: true,
    value: state,
  });
  document.dispatchEvent(new Event("visibilitychange"));
}

describe("WakeLockService", () => {
  let originalWakeLock: unknown;
  const services: WakeLockService[] = [];

  function makeService() {
    const service = new WakeLockService();
    services.push(service);
    return service;
  }

  beforeEach(() => {
    originalWakeLock = (navigator as unknown as { wakeLock?: unknown }).wakeLock;
  });

  afterEach(() => {
    while (services.length > 0) services.pop()?.dispose();
    Object.defineProperty(navigator, "wakeLock", {
      configurable: true,
      value: originalWakeLock,
    });
    fireVisibilityChange("visible");
  });

  it("requests a screen wake lock when both routing and the setting are on", async () => {
    const handle = installFakeWakeLock();
    const service = makeService();
    await service.update({ keepScreenOn: true, isRouting: true });
    expect(handle.fake.request).toHaveBeenCalledWith("screen");
  });

  it("does not request when only routing is on but the setting is off", async () => {
    const handle = installFakeWakeLock();
    const service = makeService();
    await service.update({ keepScreenOn: false, isRouting: true });
    expect(handle.fake.request).not.toHaveBeenCalled();
  });

  it("does not request when the setting is on but no route is active", async () => {
    const handle = installFakeWakeLock();
    const service = makeService();
    await service.update({ keepScreenOn: true, isRouting: false });
    expect(handle.fake.request).not.toHaveBeenCalled();
  });

  it("releases the lock when routing stops", async () => {
    const handle = installFakeWakeLock();
    const service = makeService();
    await service.update({ keepScreenOn: true, isRouting: true });
    const sentinel = handle.last as unknown as FakeSentinel;
    await service.update({ keepScreenOn: true, isRouting: false });
    expect(sentinel.release).toHaveBeenCalled();
  });

  it("releases the lock when keepScreenOn is toggled off", async () => {
    const handle = installFakeWakeLock();
    const service = makeService();
    await service.update({ keepScreenOn: true, isRouting: true });
    const sentinel = handle.last as unknown as FakeSentinel;
    await service.update({ keepScreenOn: false, isRouting: true });
    expect(sentinel.release).toHaveBeenCalled();
  });

  it("re-requests the lock when the page becomes visible again while still routing+enabled", async () => {
    const handle = installFakeWakeLock();
    const service = makeService();
    await service.update({ keepScreenOn: true, isRouting: true });
    expect(handle.fake.request).toHaveBeenCalledTimes(1);
    fireVisibilityChange("hidden");
    fireVisibilityChange("visible");
    await new Promise((resolve) => setTimeout(resolve, 0));
    expect(handle.fake.request).toHaveBeenCalledTimes(2);
  });

  it("is a no-op on browsers without navigator.wakeLock", async () => {
    Object.defineProperty(navigator, "wakeLock", {
      configurable: true,
      value: undefined,
    });
    const service = makeService();
    await expect(service.update({ keepScreenOn: true, isRouting: true })).resolves.toBeUndefined();
  });
});
