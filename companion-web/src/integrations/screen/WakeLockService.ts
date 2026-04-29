// Browser Wake Lock wrapper. Acquires a `screen` lock while routing is active
// and the keepScreenOn setting is on. Re-acquires on page visibility because
// the platform auto-releases the lock when the tab is hidden.

type WakeLockSentinelLike = {
  release(): Promise<void>;
};

type WakeLockApi = {
  request(type: "screen"): Promise<WakeLockSentinelLike>;
};

export type WakeLockInputs = {
  keepScreenOn: boolean;
  isRouting: boolean;
};

export class WakeLockService {
  private sentinel: WakeLockSentinelLike | undefined;
  private current: WakeLockInputs = { keepScreenOn: false, isRouting: false };
  private disposed = false;

  constructor() {
    if (typeof document !== "undefined") {
      document.addEventListener("visibilitychange", this.onVisibilityChange);
    }
  }

  async update(inputs: WakeLockInputs): Promise<void> {
    this.current = inputs;
    const shouldHold = inputs.keepScreenOn && inputs.isRouting;
    if (shouldHold && !this.sentinel) {
      await this.acquire();
    } else if (!shouldHold && this.sentinel) {
      await this.release();
    }
  }

  dispose(): void {
    this.disposed = true;
    if (typeof document !== "undefined") {
      document.removeEventListener("visibilitychange", this.onVisibilityChange);
    }
    void this.release();
  }

  private onVisibilityChange = async (): Promise<void> => {
    if (this.disposed) return;
    if (document.visibilityState !== "visible") return;
    if (!this.current.keepScreenOn || !this.current.isRouting) return;
    // The platform auto-releases the sentinel when the page hides; clear our
    // cached reference so the next acquire actually issues a new request.
    this.sentinel = undefined;
    await this.acquire();
  };

  private async acquire(): Promise<void> {
    const api = (navigator as unknown as { wakeLock?: WakeLockApi }).wakeLock;
    if (!api) return;
    try {
      this.sentinel = await api.request("screen");
    } catch {
      this.sentinel = undefined;
    }
  }

  private async release(): Promise<void> {
    const sentinel = this.sentinel;
    this.sentinel = undefined;
    if (!sentinel) return;
    try {
      await sentinel.release();
    } catch {
      /* ignore */
    }
  }
}
