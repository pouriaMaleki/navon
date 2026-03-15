import type { GesturePoint, RawTouchContact, RawTouchPhase, WasmRuntimeState } from "../types";
import {
  type ActiveTouchContact,
  buildRuntimeTouchFrame,
  endWheelPinch,
  type LogicalRect,
  mapClientToLogicalPoint,
  nextWheelPinchState,
  TOUCH_ID_OFFSET,
  type WheelPinchState,
} from "./touchStore.helpers";

type ContactState = {
  point: GesturePoint;
  pressure: number | null;
};

const WHEEL_PINCH_IDLE_TIMEOUT_MS = 140;

export class TouchStore {
  private canvas: HTMLCanvasElement | null = null;
  private customState: WasmRuntimeState | null = null;
  private pointerContacts = new Map<number, ContactState>();
  private touchContacts = new Map<number, ContactState>();
  private syntheticContacts = new Map<number, ContactState>();
  private syntheticPinch: WheelPinchState | null = null;
  private wheelPinchTimeoutId: number | null = null;
  private touchSequence = 0;

  private readonly onPointerDown = (ev: PointerEvent): void => {
    if (!this.canvas || !this.isPointerDriven(ev)) {
      return;
    }
    ev.preventDefault();
    try {
      this.canvas.setPointerCapture(ev.pointerId);
    } catch {
      // Pointer capture is optional for drag continuity.
    }
    const point = this.toLocalPoint(ev.clientX, ev.clientY);
    this.pointerContacts.set(ev.pointerId, {
      point,
      pressure: ev.pressure || null,
    });
    this.publishChangedContacts([
      {
        id: ev.pointerId,
        phase: "started",
        xPx: point.x,
        yPx: point.y,
        pressure: ev.pressure || null,
      },
    ]);
  };

  private readonly onPointerMove = (ev: PointerEvent): void => {
    if (!this.customState || !this.isPointerDriven(ev) || !this.pointerContacts.has(ev.pointerId)) {
      return;
    }
    ev.preventDefault();
    const previous = this.pointerContacts.get(ev.pointerId);
    if (!previous) {
      return;
    }
    const point = this.toLocalPoint(ev.clientX, ev.clientY);
    const pressure = ev.pressure || previous.pressure;
    this.pointerContacts.set(ev.pointerId, { point, pressure });
    const phase: RawTouchPhase =
      this.isActiveDrag(ev) && (point.x !== previous.point.x || point.y !== previous.point.y)
        ? "moved"
        : "stationary";
    this.publishChangedContacts([
      {
        id: ev.pointerId,
        phase,
        xPx: point.x,
        yPx: point.y,
        pressure,
      },
    ]);
  };

  private readonly onPointerRelease = (ev: PointerEvent): void => {
    if (!this.isPointerDriven(ev)) {
      return;
    }
    const previous = this.pointerContacts.get(ev.pointerId);
    const point = this.toLocalPoint(ev.clientX, ev.clientY, previous?.point);
    const phase: RawTouchPhase = ev.type === "pointercancel" ? "cancelled" : "ended";
    const pressure = ev.pressure || previous?.pressure || null;
    this.pointerContacts.delete(ev.pointerId);
    this.publishChangedContacts([
      {
        id: ev.pointerId,
        phase,
        xPx: point.x,
        yPx: point.y,
        pressure,
      },
    ]);
  };

  private readonly onTouchStart = (ev: TouchEvent): void => {
    if (!this.customState) {
      return;
    }
    ev.preventDefault();
    const changedContacts = Array.from(ev.changedTouches, (touch) => {
      const id = TOUCH_ID_OFFSET + touch.identifier;
      const point = this.toLocalPoint(touch.clientX, touch.clientY);
      const pressure = normalizeTouchForce(touch.force);
      this.touchContacts.set(id, { point, pressure });
      return {
        id,
        phase: "started" as const,
        xPx: point.x,
        yPx: point.y,
        pressure,
      };
    });
    this.publishChangedContacts(changedContacts);
  };

  private readonly onTouchMove = (ev: TouchEvent): void => {
    if (!this.customState) {
      return;
    }
    ev.preventDefault();
    const changedContacts = Array.from(ev.changedTouches, (touch) => {
      const id = TOUCH_ID_OFFSET + touch.identifier;
      const previous = this.touchContacts.get(id);
      const point = this.toLocalPoint(touch.clientX, touch.clientY, previous?.point);
      const pressure = normalizeTouchForce(touch.force) ?? previous?.pressure ?? null;
      this.touchContacts.set(id, { point, pressure });
      const phase: RawTouchPhase =
        previous && (point.x !== previous.point.x || point.y !== previous.point.y)
          ? "moved"
          : "stationary";
      return {
        id,
        phase,
        xPx: point.x,
        yPx: point.y,
        pressure,
      };
    });
    this.publishChangedContacts(changedContacts);
  };

  private readonly onTouchRelease = (ev: TouchEvent): void => {
    if (!this.customState) {
      return;
    }
    ev.preventDefault();
    const phase: RawTouchPhase = ev.type === "touchcancel" ? "cancelled" : "ended";
    const changedContacts = Array.from(ev.changedTouches, (touch) => {
      const id = TOUCH_ID_OFFSET + touch.identifier;
      const previous = this.touchContacts.get(id);
      const point = this.toLocalPoint(touch.clientX, touch.clientY, previous?.point);
      const pressure = normalizeTouchForce(touch.force) ?? previous?.pressure ?? null;
      this.touchContacts.delete(id);
      return {
        id,
        phase,
        xPx: point.x,
        yPx: point.y,
        pressure,
      };
    });
    this.publishChangedContacts(changedContacts);
  };

  private readonly onWheel = (ev: WheelEvent): void => {
    if (!this.customState) {
      return;
    }
    ev.preventDefault();
    const center = this.toLocalPoint(ev.clientX, ev.clientY);
    const update = nextWheelPinchState(this.syntheticPinch, center, ev.deltaY);
    this.syntheticPinch = update.state;
    this.syntheticContacts = new Map(
      update.activeContacts.map((contact) => [
        contact.id,
        { point: contact.point, pressure: contact.pressure },
      ]),
    );
    this.publishChangedContacts(update.changedContacts);
    this.resetWheelPinchTimer();
  };

  bind(canvas: HTMLCanvasElement, customState: WasmRuntimeState): void {
    this.dispose();
    this.canvas = canvas;
    this.customState = customState;
    canvas.addEventListener("pointerdown", this.onPointerDown);
    canvas.addEventListener("pointermove", this.onPointerMove);
    canvas.addEventListener("pointerup", this.onPointerRelease);
    canvas.addEventListener("pointercancel", this.onPointerRelease);
    canvas.addEventListener("touchstart", this.onTouchStart, { passive: false });
    canvas.addEventListener("touchmove", this.onTouchMove, { passive: false });
    canvas.addEventListener("touchend", this.onTouchRelease, { passive: false });
    canvas.addEventListener("touchcancel", this.onTouchRelease, { passive: false });
    canvas.addEventListener("wheel", this.onWheel, { passive: false });
  }

  dispose(): void {
    if (this.canvas) {
      this.canvas.removeEventListener("pointerdown", this.onPointerDown);
      this.canvas.removeEventListener("pointermove", this.onPointerMove);
      this.canvas.removeEventListener("pointerup", this.onPointerRelease);
      this.canvas.removeEventListener("pointercancel", this.onPointerRelease);
      this.canvas.removeEventListener("touchstart", this.onTouchStart);
      this.canvas.removeEventListener("touchmove", this.onTouchMove);
      this.canvas.removeEventListener("touchend", this.onTouchRelease);
      this.canvas.removeEventListener("touchcancel", this.onTouchRelease);
      this.canvas.removeEventListener("wheel", this.onWheel);
    }
    if (this.wheelPinchTimeoutId !== null) {
      window.clearTimeout(this.wheelPinchTimeoutId);
      this.wheelPinchTimeoutId = null;
    }
    this.canvas = null;
    this.customState = null;
    this.pointerContacts.clear();
    this.touchContacts.clear();
    this.syntheticContacts.clear();
    this.syntheticPinch = null;
    this.touchSequence = 0;
  }

  private isPointerDriven(ev: PointerEvent): boolean {
    return ev.pointerType === "mouse" || ev.pointerType === "pen";
  }

  private isActiveDrag(ev: PointerEvent): boolean {
    return ev.pointerType === "pen" ? ev.buttons !== 0 : (ev.buttons & 1) === 1;
  }

  private toLocalPoint(clientX: number, clientY: number, fallback?: GesturePoint): GesturePoint {
    if (!this.canvas) {
      return fallback ?? { x: 0, y: 0 };
    }
    const rect = this.canvas.getBoundingClientRect();
    return mapClientToLogicalPoint(
      clientX,
      clientY,
      rectToLogicalRect(rect),
      this.canvas.width,
      this.canvas.height,
    );
  }

  private activeContacts(): ActiveTouchContact[] {
    return [
      ...mapToActiveContacts(this.pointerContacts),
      ...mapToActiveContacts(this.touchContacts),
      ...mapToActiveContacts(this.syntheticContacts),
    ].sort((a, b) => a.id - b.id);
  }

  private publishChangedContacts(changedContacts: RawTouchContact[]): void {
    if (!this.customState || changedContacts.length === 0) {
      return;
    }
    this.touchSequence += 1;
    const { frame, activeTouch } = buildRuntimeTouchFrame(
      this.touchSequence,
      this.activeContacts(),
      changedContacts,
    );
    this.customState.pendingTouchFrames.push(frame);
    this.customState.activeTouch = activeTouch;
  }

  private resetWheelPinchTimer(): void {
    if (this.wheelPinchTimeoutId !== null) {
      window.clearTimeout(this.wheelPinchTimeoutId);
    }
    this.wheelPinchTimeoutId = window.setTimeout(() => {
      this.finishWheelPinch();
    }, WHEEL_PINCH_IDLE_TIMEOUT_MS);
  }

  private finishWheelPinch(): void {
    if (!this.syntheticPinch) {
      return;
    }
    const changedContacts = endWheelPinch(this.syntheticPinch);
    this.syntheticPinch = null;
    this.syntheticContacts.clear();
    this.publishChangedContacts(changedContacts);
    if (this.wheelPinchTimeoutId !== null) {
      window.clearTimeout(this.wheelPinchTimeoutId);
      this.wheelPinchTimeoutId = null;
    }
  }
}

function mapToActiveContacts(contacts: Map<number, ContactState>): ActiveTouchContact[] {
  return [...contacts.entries()].map(([id, contact]) => ({
    id,
    point: contact.point,
    pressure: contact.pressure,
  }));
}

function rectToLogicalRect(rect: DOMRect): LogicalRect {
  return {
    left: rect.left,
    top: rect.top,
    width: rect.width,
    height: rect.height,
  };
}

function normalizeTouchForce(force: number): number | null {
  if (!Number.isFinite(force) || force <= 0) {
    return null;
  }
  return force;
}
