export type Point = { x: number; y: number };

export type ScreenProfile = {
  id: string;
  label: string;
  width: number;
  height: number;
};

export type EmulatorTime = {
  tick: number;
  dtMs: number;
  totalMs: number;
};

export type EmulatorState<TCustom> = {
  profile: ScreenProfile;
  custom: TCustom;
  time: EmulatorTime;
};

export type RenderProgram<TCustom> = {
  init(state: EmulatorState<TCustom>): void;
  update(state: EmulatorState<TCustom>): void;
  render(state: EmulatorState<TCustom>, surface: import("./framebuffer").FrameBuffer): void;
};
