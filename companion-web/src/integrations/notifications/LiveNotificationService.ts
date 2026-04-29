// Single self-updating Notification used as the web equivalent of a
// lock-screen live activity. Replaces the prior notification on update by
// reusing a stable tag — the platform de-dupes by tag so the user only sees
// one entry that always reflects the latest guidance state.

const TAG = "esp32-routing";

export type NotificationContent = {
  title: string;
  body: string;
};

export class LiveNotificationService {
  private current: Notification | undefined;
  private permissionRequested = false;
  private lastPermission: NotificationPermission | undefined;

  async start(content: NotificationContent): Promise<void> {
    const granted = await this.ensurePermission();
    if (!granted) return;
    this.post(content);
  }

  async update(content: NotificationContent): Promise<void> {
    if (this.lastPermission !== "granted") return;
    this.post(content);
  }

  stop(): void {
    if (this.current) {
      try {
        this.current.close();
      } catch {
        /* ignore */
      }
      this.current = undefined;
    }
  }

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
    } catch {
      this.current = undefined;
    }
  }
}
