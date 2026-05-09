import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { WakeLockService } from "../integrations/screen/WakeLockService.js";

type FakeSentinel = {
  released: boolean;
  release: () => Promise<void>;
  /** MDN spec: the sentinel emits a 'release' event when the OS / browser
   * auto-releases the lock (e.g., visibility change, system idle). */
  addEventListener: (type: string, listener: () => void) => void;
  removeEventListener: (type: string, listener: () => void) => void;
  dispatchRelease: () => void;
};

function makeFakeSentinel(): FakeSentinel {
  const listeners: Record<string, Set<() => void>> = {};
  const sentinel: FakeSentinel = {
    released: false,
    release: vi.fn(async () => {
      sentinel.released = true;
    }),
    addEventListener: vi.fn((type, listener) => {
      if (!listeners[type]) listeners[type] = new Set();
      listeners[type].add(listener);
    }),
    removeEventListener: vi.fn((type, listener) => {
      listeners[type]?.delete(listener);
    }),
    dispatchRelease: () => {
      sentinel.released = true;
      for (const l of listeners["release"] ?? []) l();
    },
  };
  return sentinel;
}

function installFakeWakeLock() {
  const sentinels: FakeSentinel[] = [];
  let nextRejection: Error | undefined;
  const fake = {
    request: vi.fn(async (_type: string) => {
      if (nextRejection) {
        const err = nextRejection;
        nextRejection = undefined;
        throw err;
      }
      const sentinel = makeFakeSentinel();
      sentinels.push(sentinel);
      return sentinel;
    }),
    rejectNext: (err: Error) => {
      nextRejection = err;
    },
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
      return sentinels[sentinels.length - 1];
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

async function flush() {
  await new Promise((resolve) => setTimeout(resolve, 0));
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
    fireVisibilityChange("visible");
  });

  afterEach(() => {
    while (services.length > 0) services.pop()?.dispose();
    Object.defineProperty(navigator, "wakeLock", {
      configurable: true,
      value: originalWakeLock,
    });
    fireVisibilityChange("visible");
    vi.restoreAllMocks();
  });

  it("requests a screen wake lock when both routing and the setting are on", async () => {
    const handle = installFakeWakeLock();
    const service = makeService();
    service.update({ keepScreenOn: true, isRouting: true });
    await flush();
    expect(handle.fake.request).toHaveBeenCalledWith("screen");
  });

  it("does not request when only routing is on but the setting is off", async () => {
    const handle = installFakeWakeLock();
    const service = makeService();
    service.update({ keepScreenOn: false, isRouting: true });
    await flush();
    expect(handle.fake.request).not.toHaveBeenCalled();
  });

  it("does not request when the setting is on but no route is active", async () => {
    const handle = installFakeWakeLock();
    const service = makeService();
    service.update({ keepScreenOn: true, isRouting: false });
    await flush();
    expect(handle.fake.request).not.toHaveBeenCalled();
  });

  it("releases the lock when routing stops", async () => {
    const handle = installFakeWakeLock();
    const service = makeService();
    service.update({ keepScreenOn: true, isRouting: true });
    await flush();
    const sentinel = handle.last;
    service.update({ keepScreenOn: true, isRouting: false });
    await flush();
    expect(sentinel.release).toHaveBeenCalled();
  });

  it("re-requests the lock when the page becomes visible again while still routing+enabled", async () => {
    const handle = installFakeWakeLock();
    const service = makeService();
    service.update({ keepScreenOn: true, isRouting: true });
    await flush();
    expect(handle.fake.request).toHaveBeenCalledTimes(1);
    fireVisibilityChange("hidden");
    fireVisibilityChange("visible");
    await flush();
    expect(handle.fake.request).toHaveBeenCalledTimes(2);
  });

  it("clears the cached sentinel when the OS fires the 'release' event (MDN auto-release)", async () => {
    // User-reported regression: real browsers auto-release the screen
    // wake lock when the tab becomes hidden, system goes idle, etc. The
    // service must listen for the sentinel's 'release' event so it
    // doesn't think the lock is still held; without that, a follow-up
    // call to update() with the same inputs short-circuits and never
    // re-requests.
    const handle = installFakeWakeLock();
    const service = makeService();
    service.update({ keepScreenOn: true, isRouting: true });
    await flush();
    const sentinel = handle.last;
    sentinel.dispatchRelease();
    expect(sentinel.addEventListener as ReturnType<typeof vi.fn>).toHaveBeenCalledWith(
      "release",
      expect.any(Function),
    );
    // After the auto-release the service must NOT think the lock is held.
    // Toggling routing off must therefore not re-call release() on the
    // already-released sentinel (MDN: subsequent release() resolves but
    // is meaningless).
    service.update({ keepScreenOn: true, isRouting: false });
    await flush();
    // sentinel.release was never called by us — the OS released it.
    expect(sentinel.release).not.toHaveBeenCalled();
  });

  it("logs a console.error with the error name + message when the request rejects", async () => {
    // User-reported: 'screen lock doesn't work on web'. The service was
    // silently catching errors, so there was no signal in the dev tools
    // about whether the API failed (HTTPS not available, browser doesn't
    // support it, Safari quirk, …). Surface the failure so the user can
    // paste the error from devtools.
    const handle = installFakeWakeLock();
    const errorSpy = vi.spyOn(console, "error").mockImplementation(() => {});
    const err = Object.assign(new Error("Document is not active"), {
      name: "NotAllowedError",
    });
    handle.fake.rejectNext(err);
    const service = makeService();
    service.update({ keepScreenOn: true, isRouting: true });
    await flush();
    expect(errorSpy).toHaveBeenCalled();
    const firstArgs = errorSpy.mock.calls[0] ?? [];
    const joined = firstArgs.map(String).join(" ");
    expect(joined).toMatch(/NotAllowedError/);
    expect(joined).toMatch(/Document is not active/);
  });

  it("logs a console.warn when navigator.wakeLock is unavailable (no HTTPS / unsupported browser)", () => {
    Object.defineProperty(navigator, "wakeLock", {
      configurable: true,
      value: undefined,
    });
    const warnSpy = vi.spyOn(console, "warn").mockImplementation(() => {});
    const service = makeService();
    service.update({ keepScreenOn: true, isRouting: true });
    expect(warnSpy).toHaveBeenCalled();
    expect(String(warnSpy.mock.calls[0]?.[0])).toMatch(/wakeLock/i);
  });
});
