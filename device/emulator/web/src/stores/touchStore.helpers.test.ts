import { describe, expect, it } from "vitest";

import {
  type ActiveTouchContact,
  buildRuntimeTouchFrame,
  endWheelPinch,
  mapClientToLogicalPoint,
  nextWheelPinchState,
  TOUCH_ID_OFFSET,
  WHEEL_PINCH_IDS,
} from "./touchStore.helpers";

describe("touchStore helpers", () => {
  it("maps css pixels into logical canvas coordinates", () => {
    const point = mapClientToLogicalPoint(
      110,
      70,
      { left: 10, top: 20, width: 200, height: 100 },
      800,
      800,
    );

    expect(point).toEqual({ x: 400, y: 400 });
  });

  it("builds multi-touch frames with stationary companions", () => {
    const activeContacts: ActiveTouchContact[] = [
      { id: TOUCH_ID_OFFSET + 1, point: { x: 100, y: 100 }, pressure: 0.4 },
      { id: TOUCH_ID_OFFSET + 2, point: { x: 200, y: 120 }, pressure: 0.5 },
    ];

    const { frame, activeTouch } = buildRuntimeTouchFrame(4, activeContacts, [
      {
        id: TOUCH_ID_OFFSET + 1,
        phase: "moved",
        xPx: 100,
        yPx: 100,
        pressure: 0.4,
      },
    ]);

    expect(frame.contacts).toEqual([
      {
        id: TOUCH_ID_OFFSET + 1,
        phase: "moved",
        xPx: 100,
        yPx: 100,
        pressure: 0.4,
      },
      {
        id: TOUCH_ID_OFFSET + 2,
        phase: "stationary",
        xPx: 200,
        yPx: 120,
        pressure: 0.5,
      },
    ]);
    expect(activeTouch?.contacts.every((contact) => contact.phase === "stationary")).toBe(true);
  });

  it("synthesizes wheel pinch start, move, and end contacts", () => {
    const started = nextWheelPinchState(null, { x: 320, y: 280 }, -120);
    const moved = nextWheelPinchState(started.state, { x: 320, y: 280 }, -120);
    const ended = endWheelPinch(moved.state);

    expect(started.changedContacts.map((contact) => contact.phase)).toEqual(["started", "started"]);
    expect(moved.changedContacts.map((contact) => contact.phase)).toEqual(["moved", "moved"]);
    expect(ended.map((contact) => contact.phase)).toEqual(["ended", "ended"]);
    expect(started.activeContacts.map((contact) => contact.id)).toEqual([...WHEEL_PINCH_IDS]);
    expect(moved.activeContacts[0]?.point.x).toBeLessThan(started.activeContacts[0]?.point.x ?? 0);
    expect(moved.activeContacts[1]?.point.x).toBeGreaterThan(
      started.activeContacts[1]?.point.x ?? 0,
    );
  });
});
