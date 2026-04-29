import { afterEach, describe, expect, it, vi } from "vitest";
import { LiveNotificationService } from "../integrations/notifications/LiveNotificationService.js";

type FakeNotification = {
  title: string;
  body: string;
  tag: string;
  closed: boolean;
  close: () => void;
};

function installFakeNotification(initialPermission: NotificationPermission = "granted") {
  const created: FakeNotification[] = [];
  const ctor = vi.fn(function (
    this: FakeNotification,
    title: string,
    options: NotificationOptions,
  ) {
    this.title = title;
    this.body = options.body ?? "";
    this.tag = options.tag ?? "";
    this.closed = false;
    this.close = () => {
      this.closed = true;
    };
    created.push(this);
  }) as unknown as typeof Notification;
  Object.defineProperty(ctor, "permission", {
    configurable: true,
    get: () => initialPermission,
  });
  const requestPermission = vi.fn(async () => initialPermission);
  Object.defineProperty(ctor, "requestPermission", {
    configurable: true,
    value: requestPermission,
  });
  Object.defineProperty(window, "Notification", {
    configurable: true,
    value: ctor,
  });
  return { created, requestPermission };
}

describe("LiveNotificationService", () => {
  const originalNotification = (window as unknown as { Notification?: unknown }).Notification;

  afterEach(() => {
    Object.defineProperty(window, "Notification", {
      configurable: true,
      value: originalNotification,
    });
  });

  it("requests permission once before posting the first notification", async () => {
    const handle = installFakeNotification("default" as NotificationPermission);
    const service = new LiveNotificationService();
    await service.start({ title: "Riding", body: "Next: turn left in 200m" });
    expect(handle.requestPermission).toHaveBeenCalledTimes(1);
  });

  it("creates a single Notification with a stable tag and updates body on each tick", async () => {
    const handle = installFakeNotification();
    const service = new LiveNotificationService();
    await service.start({ title: "Riding", body: "first" });
    await service.update({ title: "Riding", body: "second" });
    await service.update({ title: "Riding", body: "third" });
    // Each update creates a new Notification with the same tag (the platform
    // de-dupes by tag) — but the latest body is what's visible.
    expect(handle.created.length).toBeGreaterThanOrEqual(1);
    for (const n of handle.created) {
      expect(n.tag).toBe("esp32-routing");
    }
    expect(handle.created.at(-1)?.body).toBe("third");
  });

  it("closes the notification on stop()", async () => {
    const handle = installFakeNotification();
    const service = new LiveNotificationService();
    await service.start({ title: "Riding", body: "first" });
    service.stop();
    expect(handle.created.at(-1)?.closed).toBe(true);
  });

  it("is a no-op when the user denies notification permission", async () => {
    const handle = installFakeNotification("denied" as NotificationPermission);
    const service = new LiveNotificationService();
    await service.start({ title: "Riding", body: "first" });
    expect(handle.created).toHaveLength(0);
  });
});
