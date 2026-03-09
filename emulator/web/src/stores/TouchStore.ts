import { MAX_ZOOM, MIN_ZOOM } from "../programs/wasmProgram";
import type { GesturePoint, WasmRuntimeState } from "../types";

const PAN_SENSITIVITY = 4;
const WHEEL_ZOOM_STEP = 0.0015;

export class TouchStore {
  private canvas: HTMLCanvasElement | null = null;
  private interactionTarget: HTMLElement | null = null;
  private customState: WasmRuntimeState | null = null;
  private pointers = new Map<number, GesturePoint>();
  private lastPinchDistance = 0;

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
    this.pointers.set(ev.pointerId, this.toLocal(ev));
    if (this.pointers.size === 2) {
      const pts = [...this.pointers.values()];
      const [a, b] = pts;
      if (!a || !b) {
        return;
      }
      this.lastPinchDistance = distance(a, b);
    }
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
    this.pointers.set(ev.pointerId, cur);

    if (this.pointers.size === 1 && this.isActiveDrag(ev)) {
      this.updatePan(cur.x - prev.x, cur.y - prev.y);
      return;
    }

    if (this.pointers.size >= 2) {
      const pts = [...this.pointers.values()];
      const [a, b] = pts;
      if (!a || !b) {
        return;
      }
      const nextDistance = distance(a, b);
      if (this.lastPinchDistance > 0) {
        const ratio = nextDistance / this.lastPinchDistance;
        this.customState.zoom = clamp(this.customState.zoom * ratio, MIN_ZOOM, MAX_ZOOM);
        this.customState.lastPanInputMs = performance.now();
      }
      this.lastPinchDistance = nextDistance;
    }
  };

  private readonly onPointerRelease = (ev: PointerEvent): void => {
    this.pointers.delete(ev.pointerId);
    if (this.pointers.size < 2) {
      this.lastPinchDistance = 0;
    }
  };

  private readonly onWheel = (ev: WheelEvent): void => {
    if (!this.customState) {
      return;
    }
    ev.preventDefault();
    const factor = Math.exp(-ev.deltaY * WHEEL_ZOOM_STEP);
    this.customState.zoom = clamp(this.customState.zoom * factor, MIN_ZOOM, MAX_ZOOM);
    this.customState.lastPanInputMs = performance.now();
  };

  bind(canvas: HTMLCanvasElement, customState: WasmRuntimeState): void {
    this.dispose();
    this.canvas = canvas;
    this.interactionTarget = canvas.parentElement ?? canvas;
    this.customState = customState;
    this.interactionTarget.addEventListener("pointerdown", this.onPointerDown);
    this.interactionTarget.addEventListener("pointermove", this.onPointerMove);
    this.interactionTarget.addEventListener("pointerup", this.onPointerRelease);
    this.interactionTarget.addEventListener("pointercancel", this.onPointerRelease);
    this.interactionTarget.addEventListener("wheel", this.onWheel, { passive: false });
  }

  dispose(): void {
    if (this.interactionTarget) {
      this.interactionTarget.removeEventListener("pointerdown", this.onPointerDown);
      this.interactionTarget.removeEventListener("pointermove", this.onPointerMove);
      this.interactionTarget.removeEventListener("pointerup", this.onPointerRelease);
      this.interactionTarget.removeEventListener("pointercancel", this.onPointerRelease);
      this.interactionTarget.removeEventListener("wheel", this.onWheel);
    }
    this.canvas = null;
    this.interactionTarget = null;
    this.customState = null;
    this.pointers.clear();
    this.lastPinchDistance = 0;
  }

  private toLocal(ev: PointerEvent): GesturePoint {
    if (!this.canvas) {
      return { x: 0, y: 0 };
    }
    const rect = this.canvas.getBoundingClientRect();
    return { x: ev.clientX - rect.left, y: ev.clientY - rect.top };
  }

  private updatePan(dx: number, dy: number): void {
    if (!this.canvas || !this.customState) {
      return;
    }
    const mapPerPixel =
      (10000 / Math.max(1, this.canvas.clientWidth * this.customState.zoom)) * PAN_SENSITIVITY;
    this.customState.panX = clamp(this.customState.panX - dx * mapPerPixel, -4800, 4800);
    this.customState.panY = clamp(this.customState.panY + dy * mapPerPixel, -4800, 4800);
    this.customState.lastPanInputMs = performance.now();
  }

  private isActiveDrag(ev: PointerEvent): boolean {
    if (ev.pointerType === "mouse") {
      return (ev.buttons & 1) === 1;
    }
    return true;
  }
}

function distance(a: GesturePoint, b: GesturePoint): number {
  return Math.hypot(a.x - b.x, a.y - b.y);
}

function clamp(value: number, min: number, max: number): number {
  return Math.max(min, Math.min(max, value));
}
