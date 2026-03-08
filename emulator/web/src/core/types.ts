export type Point = { x: number; y: number };

export type Line = {
  from: Point;
  to: Point;
  intensity: number;
  thickness: number;
};

export type WorldBounds = {
  minX: number;
  maxX: number;
  minY: number;
  maxY: number;
};

export type ScreenProfile = {
  id: string;
  label: string;
  width: number;
  height: number;
};

export type PointerState = {
  x: number;
  y: number;
  down: boolean;
};

export type InputSnapshot = {
  primary: PointerState;
};

export type EmulatorTime = {
  tick: number;
  dtMs: number;
  totalMs: number;
};

export type EmulatorState<TCustom> = {
  profile: ScreenProfile;
  input: InputSnapshot;
  custom: TCustom;
  time: EmulatorTime;
};

export type RenderProgram<TCustom> = {
  init(state: EmulatorState<TCustom>): void;
  update(state: EmulatorState<TCustom>): void;
  render(state: EmulatorState<TCustom>, surface: import("./framebuffer").FrameBuffer): void;
  handleInput?(state: EmulatorState<TCustom>): void;
};
