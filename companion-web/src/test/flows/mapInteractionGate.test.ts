import { describe, expect, it } from "vitest";
import { MapInteractionGate } from "../../features/home/MapInteractionGate.js";

/**
 * MapLibre fires `dragstart`/`zoomstart`/`rotatestart`/`pitchstart` even for
 * programmatic `easeTo`/`fitBounds` animations. If the MapSurface blindly
 * treats every one of those as a user gesture, it schedules
 * `noteUserMapInteraction`, which after the inactivity timeout re-issues
 * a recenter — a feedback loop that broke compass-lock persistence
 * (the '🧭 reverts after 1.3s' bug).
 *
 * MapInteractionGate is the tiny seam that records programmatic moves and
 * suppresses the MapLibre events that fall inside the quiet window.
 */
describe("MapInteractionGate (feedback-loop regression for '🧭 reverts after 1.3s')", () => {
  it("treats an event inside the quiet window after a programmatic move as NOT a user event", () => {
    const gate = new MapInteractionGate(600);
    gate.recordProgrammaticMove(1000);
    expect(
      gate.isLikelyUserEvent(1300),
      "events 300 ms after a programmatic animation must be treated as the MapLibre echo, not a user gesture",
    ).toBe(false);
  });

  it("treats an event beyond the quiet window as a user event", () => {
    const gate = new MapInteractionGate(600);
    gate.recordProgrammaticMove(1000);
    expect(gate.isLikelyUserEvent(1800)).toBe(true);
  });

  it("events before any programmatic move are user events", () => {
    const gate = new MapInteractionGate(600);
    expect(gate.isLikelyUserEvent(500)).toBe(true);
  });

  it("recording another programmatic move extends the quiet window", () => {
    const gate = new MapInteractionGate(600);
    gate.recordProgrammaticMove(1000);
    // At t=1500 the first window has expired; a second programmatic move
    // there resets the quiet window.
    gate.recordProgrammaticMove(1500);
    expect(gate.isLikelyUserEvent(1800)).toBe(false);
    expect(gate.isLikelyUserEvent(2200)).toBe(true);
  });
});
