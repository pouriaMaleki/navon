import type { GesturePoint, RawTouchContact, RawTouchPhase, WasmRuntimeState } from "../types";

type PointerState = {
  point: GesturePoint;
  pressure: number | null;
};

export class TouchStore {
  private canvas: HTMLCanvasElement | null = null;
  private interactionTarget: HTMLElement | null = null;
  private customState: WasmRuntimeState | null = null;
  private pointers = new Map<number, PointerState>();
  private touchSequence = 0;

  private readonly onPointerDown = (ev: PointerEvent): void => {
    if (!this.canvas) {
      return;
    }
    ev.preventDefault();
    const captureTarget = ev.target instanceof Element ? ev.target : this.interactionTarget;
    try {
      captureTarget?.setPointerCapture(ev.pointerId);
    } catch {
      // Pointer capture is optional for gesture tracking.
    }
    const p = this.toLocal(ev);
    this.pointers.set(ev.pointerId, {
      point: p,
      pressure: ev.pressure || null,
    });
    this.publishContact(ev.pointerId, p, "started", ev.pressure || null);
  };

  private readonly onPointerMove = (ev: PointerEvent): void => {
    if (!this.customState || !this.pointers.has(ev.pointerId)) {
      return;
    }
    ev.preventDefault();
    const prev = this.pointers.get(ev.pointerId);
    if (!prev) {
      return;
    }
    const cur = this.toLocal(ev);
    this.pointers.set(ev.pointerId, {
      point: cur,
      pressure: ev.pressure || prev.pressure,
    });
    const phase: RawTouchPhase =
      this.isActiveDrag(ev) && (cur.x !== prev.point.x || cur.y !== prev.point.y)
        ? "moved"
        : "stationary";
    this.publishContact(ev.pointerId, cur, phase, ev.pressure || prev.pressure);
  };

  private readonly onPointerRelease = (ev: PointerEvent): void => {
    const prev = this.pointers.get(ev.pointerId);
    const point = this.toLocal(ev);
    const phase: RawTouchPhase = ev.type === "pointercancel" ? "cancelled" : "ended";
    this.publishContact(ev.pointerId, point, phase, ev.pressure || prev?.pressure || null);
    this.pointers.delete(ev.pointerId);
  };

  bind(canvas: HTMLCanvasElement, customState: WasmRuntimeState): void {
    this.dispose();
    this.canvas = canvas;
    this.interactionTarget = canvas;
    this.customState = customState;
    this.interactionTarget.addEventListener("pointerdown", this.onPointerDown);
    this.interactionTarget.addEventListener("pointermove", this.onPointerMove);
    this.interactionTarget.addEventListener("pointerup", this.onPointerRelease);
    this.interactionTarget.addEventListener("pointercancel", this.onPointerRelease);
  }

  dispose(): void {
    if (this.interactionTarget) {
      this.interactionTarget.removeEventListener("pointerdown", this.onPointerDown);
      this.interactionTarget.removeEventListener("pointermove", this.onPointerMove);
      this.interactionTarget.removeEventListener("pointerup", this.onPointerRelease);
      this.interactionTarget.removeEventListener("pointercancel", this.onPointerRelease);
    }
    this.canvas = null;
    this.interactionTarget = null;
    this.customState = null;
    this.pointers.clear();
    this.touchSequence = 0;
  }

  private toLocal(ev: PointerEvent): GesturePoint {
    if (!this.canvas) {
      return { x: 0, y: 0 };
    }
    const rect = this.canvas.getBoundingClientRect();
    return { x: ev.clientX - rect.left, y: ev.clientY - rect.top };
  }

  private isActiveDrag(ev: PointerEvent): boolean {
    if (ev.pointerType === "mouse") {
      return (ev.buttons & 1) === 1;
    }
    return true;
  }

  private publishContact(
    id: number,
    point: GesturePoint,
    phase: RawTouchPhase,
    pressure: number | null,
  ): void {
    if (!this.customState) {
      return;
    }
    this.touchSequence += 1;
    const nextContacts = new Map<number, RawTouchContact>(
      this.customState.touch.contacts.map((contact) => [contact.id, contact]),
    );
    nextContacts.set(id, {
      id,
      phase,
      xPx: point.x,
      yPx: point.y,
      pressure,
    });
    this.customState.touch.sequence = this.touchSequence;
    this.customState.touch.contacts = [...nextContacts.values()].sort((a, b) => a.id - b.id);
  }
}
