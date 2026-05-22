import { beforeEach, describe, expect, it } from "vitest";
import { LocalStoragePersistence } from "../integrations/persistence/LocalStoragePersistence.js";
import { GuidanceStore } from "./GuidanceStore.js";
import { LocationStore } from "./LocationStore.js";
import { PlanningStore, type ProvidersMap } from "./PlanningStore.js";
import { SettingsStore } from "./SettingsStore.js";
import { FakeLocationService, FakePlaceSearch, FakeRoutingAdapter } from "../__testlib__/fakes/index.js";

// User feedback: when the rider keeps drifting off-route, the planner spams
// reroute attempts. After the 3rd attempt in a short window, hold the next
// auto-reroute for 5 s — and 10 s if the burst keeps growing — but always
// expose a "Reroute now" override so the rider isn't stranded.
//
// Throttle policy (sliding 30 s window):
//   attempts <= 2  → 0 ms delay (immediate)
//   attempts == 3 or 4 → 5_000 ms delay
//   attempts >= 5  → 10_000 ms delay
//
// `recordReroutingAttempt(now)` is the pure entry point — it pushes the
// timestamp, computes the delay, and exposes `reroutingDelayedUntilMs` /
// `isWaitingToReroute`. Tests pass an explicit `now` so timing is deterministic.

function buildHarness() {
  globalThis.localStorage?.clear();
  const persistence = new LocalStoragePersistence();
  const settings = new SettingsStore(persistence);
  const location = new LocationStore(new FakeLocationService(), persistence);
  const providers: ProvidersMap = {
    hsl: new FakeRoutingAdapter("hsl"),
    osm: new FakeRoutingAdapter("osm"),
    gpxImport: new FakeRoutingAdapter("gpxImport"),
    fitImport: new FakeRoutingAdapter("fitImport"),
    tcxImport: new FakeRoutingAdapter("tcxImport"),
  } as ProvidersMap;
  const planning = new PlanningStore(providers, new FakePlaceSearch(), location, settings);
  const guidance = new GuidanceStore(planning, persistence, location);
  return { guidance };
}

describe("GuidanceStore — rerouting backoff (sliding 30 s window)", () => {
  beforeEach(() => globalThis.localStorage?.clear());

  it("returns 0 ms delay for the 1st reroute attempt (no throttle yet)", () => {
    const { guidance } = buildHarness();
    expect(guidance.recordReroutingAttempt(1_000)).toBe(0);
  });

  it("returns 0 ms delay for the 2nd attempt within the window", () => {
    const { guidance } = buildHarness();
    guidance.recordReroutingAttempt(1_000);
    expect(guidance.recordReroutingAttempt(2_000)).toBe(0);
  });

  it("returns 5_000 ms delay starting at the 3rd attempt within 30 s", () => {
    const { guidance } = buildHarness();
    guidance.recordReroutingAttempt(1_000);
    guidance.recordReroutingAttempt(2_000);
    expect(guidance.recordReroutingAttempt(3_000)).toBe(5_000);
  });

  it("returns 5_000 ms delay for the 4th attempt", () => {
    const { guidance } = buildHarness();
    guidance.recordReroutingAttempt(1_000);
    guidance.recordReroutingAttempt(2_000);
    guidance.recordReroutingAttempt(3_000);
    expect(guidance.recordReroutingAttempt(4_000)).toBe(5_000);
  });

  it("escalates to 10_000 ms delay at the 5th attempt within 30 s", () => {
    const { guidance } = buildHarness();
    guidance.recordReroutingAttempt(1_000);
    guidance.recordReroutingAttempt(2_000);
    guidance.recordReroutingAttempt(3_000);
    guidance.recordReroutingAttempt(4_000);
    expect(guidance.recordReroutingAttempt(5_000)).toBe(10_000);
  });

  it("resets the counter once attempts age out of the 30 s window", () => {
    const { guidance } = buildHarness();
    // Five attempts at 0–4 s — backoff is now at 10 s.
    for (let i = 0; i < 5; i += 1) guidance.recordReroutingAttempt(i * 1_000);
    // After 30 s have passed, all five attempts have aged out: next attempt
    // is the "1st" again with no delay.
    expect(guidance.recordReroutingAttempt(35_000)).toBe(0);
  });

  it("sets isWaitingToReroute to true while inside the throttle window", () => {
    const { guidance } = buildHarness();
    guidance.recordReroutingAttempt(1_000);
    guidance.recordReroutingAttempt(2_000);
    guidance.recordReroutingAttempt(3_000);
    expect(guidance.isWaitingToReroute(3_000)).toBe(true);
    expect(guidance.reroutingDelayedUntilMs).toBe(8_000); // 3_000 + 5_000
  });

  it("clears the waiting state when requestManualReroute is invoked", () => {
    const { guidance } = buildHarness();
    guidance.recordReroutingAttempt(1_000);
    guidance.recordReroutingAttempt(2_000);
    guidance.recordReroutingAttempt(3_000);
    expect(guidance.isWaitingToReroute(3_500)).toBe(true);
    guidance.requestManualReroute();
    expect(guidance.isWaitingToReroute(3_500)).toBe(false);
    expect(guidance.reroutingDelayedUntilMs).toBeUndefined();
  });

  it("isWaitingToReroute returns false once the delay elapses", () => {
    const { guidance } = buildHarness();
    guidance.recordReroutingAttempt(1_000);
    guidance.recordReroutingAttempt(2_000);
    guidance.recordReroutingAttempt(3_000);
    // Delay is 5 s → ready at 8_000
    expect(guidance.isWaitingToReroute(7_999)).toBe(true);
    expect(guidance.isWaitingToReroute(8_000)).toBe(false);
  });

  it("markAutoRerouteDispatched re-arms reroute while staying off-route", () => {
    const { guidance } = buildHarness();
    guidance.offRoute = true;
    guidance.recordReroutingAttempt(1_000);
    guidance.recordReroutingAttempt(2_000);
    guidance.recordReroutingAttempt(3_000);
    guidance.rerouteRequested = true;

    guidance.markAutoRerouteDispatched();

    expect(guidance.rerouteRequested).toBe(false);
    expect(guidance.reroutingDelayedUntilMs).toBeUndefined();
  });
});
