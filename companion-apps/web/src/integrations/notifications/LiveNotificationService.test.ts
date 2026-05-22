import { afterEach, describe, expect, it, vi } from "vitest";
import { LiveNotificationService } from "./LiveNotificationService.js";

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

function setVisibility(state: "visible" | "hidden") {
  Object.defineProperty(document, "visibilityState", {
    configurable: true,
    value: state,
  });
  document.dispatchEvent(new Event("visibilitychange"));
}

describe("LiveNotificationService", () => {
  const originalNotification = (window as unknown as { Notification?: unknown }).Notification;

  afterEach(() => {
    Object.defineProperty(window, "Notification", {
      configurable: true,
      value: originalNotification,
    });
    setVisibility("visible");
  });

  it("requests permission once before posting the first notification", async () => {
    const handle = installFakeNotification("default" as NotificationPermission);
    const service = new LiveNotificationService();
    setVisibility("hidden");
    await service.start({ title: "Riding", body: "Turn left" });
    expect(handle.requestPermission).toHaveBeenCalledTimes(1);
    service.dispose();
  });

  it("does NOT post a notification when the document is visible (foregrounded)", async () => {
    // User-reported regression: routing kept firing a notification chime
    // every few seconds because every guidance tick called update()
    // regardless of whether the user could already see the app on
    // screen. The platform Notification is only useful when the page
    // is hidden — visible page = no notification.
    const handle = installFakeNotification();
    const service = new LiveNotificationService();
    setVisibility("visible");
    await service.start({ title: "Riding", body: "Turn left" });
    await service.update({ title: "Riding", body: "Turn left" });
    await service.update({ title: "Riding", body: "Turn right" });
    expect(handle.created).toHaveLength(0);
    service.dispose();
  });

  it("posts the latest pending content when the document becomes hidden", async () => {
    const handle = installFakeNotification();
    const service = new LiveNotificationService();
    setVisibility("visible");
    await service.start({ title: "Riding", body: "Turn left" });
    await service.update({ title: "Riding", body: "Turn right" });
    expect(handle.created).toHaveLength(0);
    setVisibility("hidden");
    expect(handle.created).toHaveLength(1);
    expect(handle.created[0].body).toBe("Turn right");
    service.dispose();
  });

  it("closes the notification when the document becomes visible again", async () => {
    const handle = installFakeNotification();
    const service = new LiveNotificationService();
    setVisibility("hidden");
    await service.start({ title: "Riding", body: "Turn left" });
    expect(handle.created).toHaveLength(1);
    setVisibility("visible");
    expect(handle.created[0].closed).toBe(true);
    service.dispose();
  });

  it("dedupes identical bodies — repeated update with same content does not chime again", async () => {
    const handle = installFakeNotification();
    const service = new LiveNotificationService();
    setVisibility("hidden");
    await service.start({ title: "Riding", body: "Turn left" });
    await service.update({ title: "Riding", body: "Turn left" });
    await service.update({ title: "Riding", body: "Turn left" });
    expect(handle.created).toHaveLength(1);
    service.dispose();
  });

  it("creates a fresh notification when the body actually changes (rider passed the next maneuver)", async () => {
    const handle = installFakeNotification();
    const service = new LiveNotificationService();
    setVisibility("hidden");
    await service.start({ title: "Riding", body: "Turn left" });
    await service.update({ title: "Riding", body: "Turn right" });
    expect(handle.created).toHaveLength(2);
    expect(handle.created[1].body).toBe("Turn right");
    expect(handle.created[0].closed).toBe(true);
    service.dispose();
  });

  it("stop() closes the notification and forgets the pending content", async () => {
    const handle = installFakeNotification();
    const service = new LiveNotificationService();
    setVisibility("hidden");
    await service.start({ title: "Riding", body: "Turn left" });
    expect(handle.created.at(-1)?.closed).toBe(false);
    service.stop();
    expect(handle.created.at(-1)?.closed).toBe(true);
    // After stop, going hidden again must NOT resurrect a phantom notification.
    setVisibility("visible");
    setVisibility("hidden");
    expect(handle.created).toHaveLength(1);
    service.dispose();
  });

  it("only posts when an upcoming turn is close (≤ 500m), not when the rider is far away", async () => {
    // User-reported: lock-screen card refreshed every few seconds with the
    // same "Continue" copy. Notifications should be sparse — only fire when
    // there's an actual upcoming turn the rider is close enough to need to
    // act on. Coordinator passes a `closeUpcomingTurn` flag; service uses
    // it to gate posts even while the page is hidden.
    const handle = installFakeNotification();
    const service = new LiveNotificationService();
    setVisibility("hidden");
    await service.start({ title: "Riding", body: "Continue", closeUpcomingTurn: false });
    expect(handle.created).toHaveLength(0);
    await service.update({ title: "Riding", body: "Turn left", closeUpcomingTurn: true });
    expect(handle.created).toHaveLength(1);
    expect(handle.created[0].body).toBe("Turn left");
    service.dispose();
  });

  it("closes an existing notification when the upcoming turn falls back to far-away", async () => {
    const handle = installFakeNotification();
    const service = new LiveNotificationService();
    setVisibility("hidden");
    await service.start({ title: "Riding", body: "Turn left", closeUpcomingTurn: true });
    expect(handle.created).toHaveLength(1);
    await service.update({ title: "Riding", body: "Continue", closeUpcomingTurn: false });
    expect(handle.created.at(-1)?.closed).toBe(true);
    service.dispose();
  });

  it("is a no-op when the user denies notification permission", async () => {
    const handle = installFakeNotification("denied" as NotificationPermission);
    const service = new LiveNotificationService();
    setVisibility("hidden");
    await service.start({ title: "Riding", body: "Turn left" });
    expect(handle.created).toHaveLength(0);
    service.dispose();
  });
});
