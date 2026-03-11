import { CanvasTarget } from "./canvasTarget";
import { FrameBuffer } from "./framebuffer";
import type { EmulatorState, InputSnapshot, RenderProgram, ScreenProfile } from "./types";

export class Esp32ScreenEmulator<TCustom> {
  private readonly target: CanvasTarget;
  private readonly surface: FrameBuffer;
  private readonly state: EmulatorState<TCustom>;
  private running = false;
  private rafId = 0;
  private prevTs = 0;
  private readonly onFrame: ((dtMs: number) => void) | undefined;

  constructor(
    canvas: HTMLCanvasElement,
    profile: ScreenProfile,
    custom: TCustom,
    private readonly program: RenderProgram<TCustom>,
    options?: {
      onFrame?: (dtMs: number) => void;
    },
  ) {
    const input: InputSnapshot = {
      primary: { x: 0, y: 0, down: false },
    };
    this.target = new CanvasTarget(canvas, profile.width, profile.height);
    this.surface = new FrameBuffer(profile.width, profile.height);
    this.state = {
      profile,
      input,
      custom,
      time: { tick: 0, dtMs: 0, totalMs: 0 },
    };
    this.installPointerInput(canvas);
    this.onFrame = options?.onFrame;
    this.program.init(this.state);
  }

  start(): void {
    if (this.running) {
      return;
    }
    this.running = true;
    this.prevTs = performance.now();
    this.rafId = requestAnimationFrame(this.step);
  }

  stop(): void {
    if (!this.running) {
      return;
    }
    this.running = false;
    cancelAnimationFrame(this.rafId);
  }

  toggle(): boolean {
    if (this.running) {
      this.stop();
    } else {
      this.start();
    }
    return this.running;
  }

  reset(): void {
    this.state.time.tick = 0;
    this.state.time.dtMs = 0;
    this.state.time.totalMs = 0;
    this.program.init(this.state);
    this.renderOnce();
  }

  renderOnce(): void {
    this.program.render(this.state, this.surface);
    this.target.present(this.surface);
  }

  customState(): TCustom {
    return this.state.custom;
  }

  private readonly step = (ts: number): void => {
    if (!this.running) {
      return;
    }
    const dtMs = Math.max(0, ts - this.prevTs);
    this.prevTs = ts;
    this.state.time.dtMs = dtMs;
    this.state.time.totalMs += dtMs;
    this.state.time.tick += 1;

    if (this.program.handleInput) {
      this.program.handleInput(this.state);
    }
    this.program.update(this.state);
    this.program.render(this.state, this.surface);
    this.target.present(this.surface);
    this.onFrame?.(dtMs);
    this.rafId = requestAnimationFrame(this.step);
  };

  private installPointerInput(canvas: HTMLCanvasElement): void {
    const updatePoint = (ev: PointerEvent): void => {
      const rect = canvas.getBoundingClientRect();
      const nx = (ev.clientX - rect.left) / Math.max(1, rect.width);
      const ny = (ev.clientY - rect.top) / Math.max(1, rect.height);
      this.state.input.primary.x = Math.max(0, Math.min(1, nx));
      this.state.input.primary.y = Math.max(0, Math.min(1, ny));
    };

    canvas.addEventListener("pointerdown", (ev) => {
      updatePoint(ev);
      this.state.input.primary.down = true;
    });
    canvas.addEventListener("pointermove", updatePoint);
    canvas.addEventListener("pointerup", (ev) => {
      updatePoint(ev);
      this.state.input.primary.down = false;
    });
    canvas.addEventListener("pointercancel", () => {
      this.state.input.primary.down = false;
    });
    canvas.addEventListener("pointerleave", () => {
      this.state.input.primary.down = false;
    });
  }
}
