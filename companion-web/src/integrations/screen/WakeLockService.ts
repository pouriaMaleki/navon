// Browser Screen Wake Lock wrapper. Acquires a `screen` lock while
// routing is active and the keepScreenOn setting is on. Re-acquires on
// page visibility because the platform auto-releases the lock when the
// tab is hidden (per MDN: Screen_Wake_Lock_API).
//
// The service follows the MDN reference impl in three important ways:
//   1. The actual `navigator.wakeLock.request("screen")` call is fired
//      synchronously (no `await` between the user-gesture-driven entry
//      point and the request) so transient user activation is preserved
//      on browsers that require it (notably Safari).
//   2. We listen for the sentinel's `'release'` event so the service
//      knows when the OS auto-releases the lock and can re-acquire on
//      the next visibility change.
//   3. Failures are surfaced via `console.error` / `console.warn`
//      instead of being silently swallowed — so when the API is missing
//      (no HTTPS, unsupported browser) or rejects (NotAllowedError,
//      page hidden, …), the developer sees the reason in devtools.

type WakeLockSentinelLike = {
  release(): Promise<void>;
  addEventListener?: (type: string, listener: () => void) => void;
  removeEventListener?: (type: string, listener: () => void) => void;
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
  private requesting = false;
  private disposed = false;
  /** Pinned listener so `removeEventListener` actually unbinds on
   * `dispose`. Stored separately because `addEventListener` accepts a
   * raw callback. */
  private readonly onReleaseHandler = this.handleSentinelRelease.bind(this);

  constructor() {
    if (typeof document !== "undefined") {
      document.addEventListener("visibilitychange", this.onVisibilityChange);
    }
  }

  update(inputs: WakeLockInputs): void {
    this.current = inputs;
    const shouldHold = inputs.keepScreenOn && inputs.isRouting;
    if (shouldHold) {
      this.acquire();
    } else if (this.sentinel) {
      this.releaseLock();
    }
  }

  dispose(): void {
    this.disposed = true;
    if (typeof document !== "undefined") {
      document.removeEventListener("visibilitychange", this.onVisibilityChange);
    }
    this.releaseLock();
  }

  private onVisibilityChange = (): void => {
    if (this.disposed) return;
    if (document.visibilityState !== "visible") return;
    if (!this.current.keepScreenOn || !this.current.isRouting) return;
    // The platform auto-releases the sentinel when the page hides; clear
    // our cached reference so the next acquire actually fires a fresh
    // request.
    this.sentinel = undefined;
    this.acquire();
  };

  private acquire(): void {
    if (this.sentinel || this.requesting || this.disposed) return;
    const api = (navigator as unknown as { wakeLock?: WakeLockApi }).wakeLock;
    if (!api) {
      // Most common reasons: page is not on HTTPS, or the browser doesn't
      // support the API (Safari < 16.4, in-app browsers, older Firefox).
      console.warn(
        "[WakeLockService] navigator.wakeLock is undefined — Screen Wake Lock requires a secure context (HTTPS) and a supported browser (Chrome/Edge ≥ 84, Firefox ≥ 126, Safari ≥ 16.4).",
      );
      return;
    }
    this.requesting = true;
    // Call .request() synchronously inside the (potentially user-gesture-
    // driven) tick. Awaiting before this line risks losing transient user
    // activation on Safari.
    api.request("screen").then(
      (sentinel) => {
        this.requesting = false;
        if (this.disposed) {
          // We were torn down between request and resolve — release the
          // lock immediately rather than holding it indefinitely.
          void sentinel.release().catch(() => {
            /* ignore */
          });
          return;
        }
        // If the user toggled keepScreenOn off / stopped routing while
        // the request was in flight, release the freshly-acquired lock.
        const stillWanted = this.current.keepScreenOn && this.current.isRouting;
        if (!stillWanted) {
          void sentinel.release().catch(() => {
            /* ignore */
          });
          return;
        }
        this.sentinel = sentinel;
        sentinel.addEventListener?.("release", this.onReleaseHandler);
      },
      (err: unknown) => {
        this.requesting = false;
        const e = err as { name?: string; message?: string };
        // NotAllowedError is the most common — the page lost user
        // activation, or the document is not visible / focused. Surface
        // both name and message so the developer can match against the
        // MDN error catalog.
        console.error(
          `[WakeLockService] navigator.wakeLock.request("screen") rejected: ${e?.name ?? "Error"}: ${e?.message ?? String(err)}`,
        );
      },
    );
  }

  private releaseLock(): void {
    const sentinel = this.sentinel;
    this.sentinel = undefined;
    if (!sentinel) return;
    sentinel.removeEventListener?.("release", this.onReleaseHandler);
    sentinel.release().catch((err: unknown) => {
      const e = err as { message?: string };
      console.warn(`[WakeLockService] release() rejected: ${e?.message ?? String(err)}`);
    });
  }

  /** Fires when the platform releases the lock (visibility change,
   * system idle, manual release elsewhere). MDN spec — see
   * `WakeLockSentinel.release` event. */
  private handleSentinelRelease(): void {
    this.sentinel = undefined;
  }
}
