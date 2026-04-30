// Single self-updating Notification used as the web equivalent of a
// lock-screen live activity.
//
// User-reported issue: leaving the live-notification toggle on caused a
// notification chime every few seconds during routing, because the
// coordinator's MobX `autorun` re-fired on every guidance tick. This
// implementation is visibility-aware: while the page is foregrounded
// the user can see the on-screen guidance card already, so we never
// post the platform Notification at all. We only render it once the
// document goes hidden, and we close it again when the user returns.
//
// We also dedupe identical bodies — turn-by-turn alerts that don't
// change between two ticks ("Turn right" → "Turn right") shouldn't
// trigger a fresh chime.
//
// Stop() closes the active notification AND forgets the pending
// content, so a later visibility flip won't resurrect a phantom card
// after routing ends.

const TAG = "esp32-routing";

export type NotificationContent = {
  title: string;
  body: string;
};

export class LiveNotificationService {
  private current: Notification | undefined;
  /** Latest content the wiring layer asked us to display. Posted on the
   * platform only while the document is hidden. */
  private latest: NotificationContent | undefined;
  /** Body of the most recently posted Notification, used to dedupe. */
  private postedBody: string | undefined;
  private permissionRequested = false;
  private lastPermission: NotificationPermission | undefined;
  private active = true;

  constructor() {
    if (typeof document !== "undefined") {
      document.addEventListener("visibilitychange", this.onVisibilityChange);
    }
  }

  async start(content: NotificationContent): Promise<void> {
    if (!this.active) return;
    this.latest = content;
    const granted = await this.ensurePermission();
    if (!granted) return;
    this.postIfHidden();
  }

  async update(content: NotificationContent): Promise<void> {
    if (!this.active) return;
    this.latest = content;
    if (this.lastPermission !== "granted") return;
    this.postIfHidden();
  }

  stop(): void {
    this.latest = undefined;
    this.postedBody = undefined;
    this.closeCurrent();
  }

  /** Releases the visibilitychange listener. Call on unmount. */
  dispose(): void {
    this.active = false;
    if (typeof document !== "undefined") {
      document.removeEventListener("visibilitychange", this.onVisibilityChange);
    }
    this.closeCurrent();
  }

  private onVisibilityChange = (): void => {
    if (typeof document === "undefined") return;
    if (document.visibilityState === "hidden") {
      this.postIfHidden();
    } else {
      // Page is visible — the user can see the on-screen guidance card,
      // the lock-screen card is redundant noise.
      this.closeCurrent();
    }
  };

  private async ensurePermission(): Promise<boolean> {
    const ctor = (window as unknown as { Notification?: typeof Notification }).Notification;
    if (!ctor) return false;
    if (ctor.permission === "granted") {
      this.lastPermission = "granted";
      return true;
    }
    if (ctor.permission === "denied") {
      this.lastPermission = "denied";
      return false;
    }
    if (this.permissionRequested) {
      return this.lastPermission === "granted";
    }
    this.permissionRequested = true;
    try {
      const result = await ctor.requestPermission();
      this.lastPermission = result;
      return result === "granted";
    } catch {
      this.lastPermission = "denied";
      return false;
    }
  }

  private postIfHidden(): void {
    if (typeof document === "undefined") return;
    if (document.visibilityState !== "hidden") return;
    const content = this.latest;
    if (!content) return;
    if (this.lastPermission !== "granted") return;
    // Dedupe: skip if the body is unchanged from the last successful post.
    if (this.current && this.postedBody === content.body) return;
    this.post(content);
  }

  private post(content: NotificationContent): void {
    const ctor = (window as unknown as { Notification?: typeof Notification }).Notification;
    if (!ctor) return;
    if (this.current) {
      try {
        this.current.close();
      } catch {
        /* ignore */
      }
    }
    try {
      this.current = new ctor(content.title, {
        body: content.body,
        tag: TAG,
      });
      this.postedBody = content.body;
    } catch {
      this.current = undefined;
      this.postedBody = undefined;
    }
  }

  private closeCurrent(): void {
    if (this.current) {
      try {
        this.current.close();
      } catch {
        /* ignore */
      }
      this.current = undefined;
      this.postedBody = undefined;
    }
  }
}
