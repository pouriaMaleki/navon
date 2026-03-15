import type { GesturePoint, RawTouchContact, RuntimeTouchInput } from "../types";

export const TOUCH_ID_OFFSET = 100_000;
export const WHEEL_PINCH_IDS = [900_001, 900_002] as const;
const DEFAULT_WHEEL_PINCH_RADIUS_PX = 42;
const MIN_WHEEL_PINCH_RADIUS_PX = 18;
const MAX_WHEEL_PINCH_RADIUS_PX = 160;
const WHEEL_PINCH_SENSITIVITY = 0.0015;

export type ActiveTouchContact = {
  id: number;
  point: GesturePoint;
  pressure: number | null;
};

export type LogicalRect = {
  left: number;
  top: number;
  width: number;
  height: number;
};

export type WheelPinchState = {
  center: GesturePoint;
  radiusPx: number;
};

export function mapClientToLogicalPoint(
  clientX: number,
  clientY: number,
  rect: LogicalRect,
  canvasWidth: number,
  canvasHeight: number,
): GesturePoint {
  if (rect.width <= 0 || rect.height <= 0) {
    return { x: 0, y: 0 };
  }
  const x = ((clientX - rect.left) * canvasWidth) / rect.width;
  const y = ((clientY - rect.top) * canvasHeight) / rect.height;
  return {
    x: clamp(x, 0, canvasWidth),
    y: clamp(y, 0, canvasHeight),
  };
}

export function buildRuntimeTouchFrame(
  sequence: number,
  activeContacts: readonly ActiveTouchContact[],
  changedContacts: readonly RawTouchContact[],
): { frame: RuntimeTouchInput; activeTouch: RuntimeTouchInput | null } {
  const changedIds = new Set(changedContacts.map((contact) => contact.id));
  const stationaryContacts = activeContacts
    .filter((contact) => !changedIds.has(contact.id))
    .map(
      (contact): RawTouchContact => ({
        id: contact.id,
        phase: "stationary",
        xPx: contact.point.x,
        yPx: contact.point.y,
        pressure: contact.pressure,
      }),
    );
  const contacts = [...changedContacts, ...stationaryContacts].sort((a, b) => a.id - b.id);
  const frame = { sequence, contacts };
  const activeTouch =
    activeContacts.length > 0
      ? {
          sequence,
          contacts: activeContacts
            .map(
              (contact): RawTouchContact => ({
                id: contact.id,
                phase: "stationary",
                xPx: contact.point.x,
                yPx: contact.point.y,
                pressure: contact.pressure,
              }),
            )
            .sort((a, b) => a.id - b.id),
        }
      : null;
  return { frame, activeTouch };
}

export function nextWheelPinchState(
  previous: WheelPinchState | null,
  center: GesturePoint,
  deltaY: number,
): {
  state: WheelPinchState;
  activeContacts: ActiveTouchContact[];
  changedContacts: RawTouchContact[];
} {
  const previousRadius = previous?.radiusPx ?? DEFAULT_WHEEL_PINCH_RADIUS_PX;
  const radiusPx = clamp(
    previousRadius * Math.exp(-deltaY * WHEEL_PINCH_SENSITIVITY),
    MIN_WHEEL_PINCH_RADIUS_PX,
    MAX_WHEEL_PINCH_RADIUS_PX,
  );
  const state = { center, radiusPx };
  const activeContacts = wheelPinchContacts(state, null);
  const phase = previous ? "moved" : "started";
  return {
    state,
    activeContacts,
    changedContacts: activeContacts.map(
      (contact): RawTouchContact => ({
        id: contact.id,
        phase,
        xPx: contact.point.x,
        yPx: contact.point.y,
        pressure: contact.pressure,
      }),
    ),
  };
}

export function endWheelPinch(state: WheelPinchState): RawTouchContact[] {
  return wheelPinchContacts(state, null).map(
    (contact): RawTouchContact => ({
      id: contact.id,
      phase: "ended",
      xPx: contact.point.x,
      yPx: contact.point.y,
      pressure: null,
    }),
  );
}

function wheelPinchContacts(state: WheelPinchState, pressure: number | null): ActiveTouchContact[] {
  return [
    {
      id: WHEEL_PINCH_IDS[0],
      point: { x: state.center.x - state.radiusPx, y: state.center.y },
      pressure,
    },
    {
      id: WHEEL_PINCH_IDS[1],
      point: { x: state.center.x + state.radiusPx, y: state.center.y },
      pressure,
    },
  ];
}

function clamp(value: number, min: number, max: number): number {
  return Math.min(max, Math.max(min, value));
}
