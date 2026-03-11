import { MAX_ZOOM, MIN_ZOOM } from "../programs/wasmProgram";
import type { GesturePoint, WasmRuntimeState } from "../types";

const PAN_SENSITIVITY = 2;
const WHEEL_ZOOM_STEP = 0.0015;
const TAP_DURATION_MS = 260;
const TAP_MOVE_PX = 10;

type PointerState = {
  point: GesturePoint;
  downPoint: GesturePoint;
  downAtMs: number;
};

export class TouchStore {
  private canvas: HTMLCanvasElement | null = null;
  private interactionTarget: HTMLElement | null = null;
  private customState: WasmRuntimeState | null = null;
  private pointers = new Map<number, PointerState>();
  private lastPinchDistance = 0;
  private lastPinchAngleRad = 0;

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
      downPoint: p,
      downAtMs: performance.now(),
    });
    if (this.pointers.size === 2) {
      const pts = [...this.pointers.values()].map((state) => state.point);
      const [a, b] = pts;
      if (!a || !b) {
        return;
      }
      this.lastPinchDistance = distance(a, b);
      this.lastPinchAngleRad = angleRad(a, b);
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
    this.pointers.set(ev.pointerId, {
      ...prev,
      point: cur,
    });

    if (this.pointers.size === 1 && this.isActiveDrag(ev)) {
      this.updatePan(cur.x - prev.point.x, cur.y - prev.point.y);
      return;
    }

    if (this.pointers.size >= 2) {
      const pts = [...this.pointers.values()].map((state) => state.point);
      const [a, b] = pts;
      if (!a || !b) {
        return;
      }
      const nextDistance = distance(a, b);
      const nextAngleRad = angleRad(a, b);
      if (this.lastPinchDistance > 0) {
        const ratio = nextDistance / this.lastPinchDistance;
        this.customState.zoom = clamp(this.customState.zoom * ratio, MIN_ZOOM, MAX_ZOOM);
        this.customState.rotateDeltaRad += normalizeAngle(nextAngleRad - this.lastPinchAngleRad);
        this.customState.lastPanInputMs = performance.now();
      }
      this.lastPinchDistance = nextDistance;
      this.lastPinchAngleRad = nextAngleRad;
    }
  };

  private readonly onPointerRelease = (ev: PointerEvent): void => {
    const prev = this.pointers.get(ev.pointerId);
    this.pointers.delete(ev.pointerId);
    if (prev && this.pointers.size === 0) {
      this.tryTapNorthIndicator(prev, this.toLocal(ev));
    }
    if (this.pointers.size < 2) {
      this.lastPinchDistance = 0;
      this.lastPinchAngleRad = 0;
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
    this.lastPinchAngleRad = 0;
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

  private tryTapNorthIndicator(state: PointerState, releasePoint: GesturePoint): void {
    if (!this.canvas || !this.customState) {
      return;
    }
    const elapsedMs = performance.now() - state.downAtMs;
    const dx = releasePoint.x - state.downPoint.x;
    const dy = releasePoint.y - state.downPoint.y;
    if (elapsedMs > TAP_DURATION_MS || Math.hypot(dx, dy) > TAP_MOVE_PX) {
      return;
    }
    const rect = this.canvas.getBoundingClientRect();
    if (rect.width <= 0 || rect.height <= 0) {
      return;
    }
    const nx = releasePoint.x / rect.width;
    const ny = releasePoint.y / rect.height;
    this.customState.emu.tap_normalized(nx, ny);
  }
}

function distance(a: GesturePoint, b: GesturePoint): number {
  return Math.hypot(a.x - b.x, a.y - b.y);
}

function angleRad(a: GesturePoint, b: GesturePoint): number {
  return Math.atan2(b.y - a.y, b.x - a.x);
}

function normalizeAngle(angle: number): number {
  const twoPi = Math.PI * 2;
  let out = angle;
  while (out > Math.PI) {
    out -= twoPi;
  }
  while (out < -Math.PI) {
    out += twoPi;
  }
  return out;
}

function clamp(value: number, min: number, max: number): number {
  return Math.max(min, Math.min(max, value));
}
