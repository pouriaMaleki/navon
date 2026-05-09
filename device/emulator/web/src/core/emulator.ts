import { CanvasTarget } from "./canvasTarget";
import { FrameBuffer } from "./framebuffer";
import type { EmulatorState, RenderProgram, ScreenProfile } from "./types";

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
    this.target = new CanvasTarget(canvas, profile.width, profile.height);
    this.surface = new FrameBuffer(profile.width, profile.height);
    this.state = {
      profile,
      custom,
      time: { tick: 0, dtMs: 0, totalMs: 0 },
    };
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
    this.program.update(this.state);
    this.program.render(this.state, this.surface);
    this.target.present(this.surface);
    this.onFrame?.(dtMs);
    this.rafId = requestAnimationFrame(this.step);
  };
}
