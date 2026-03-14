import initWasm, { type InitOutput, MinimapWasmEmulator } from "../../wasm-pkg/render_core_wasm.js";
import type { RenderProgram } from "../core/types";

import type { WasmRuntimeState } from "../types";

export async function createWasmProgram(profileId = 0): Promise<{
  initialState: WasmRuntimeState;
  program: RenderProgram<WasmRuntimeState>;
}> {
  const wasm: InitOutput = await initWasm();
  const emu = new MinimapWasmEmulator(profileId);

  const program: RenderProgram<WasmRuntimeState> = {
    init(state) {
      state.custom.emu.reset();
      state.custom.frame = null;
      state.custom.touch.sequence = 0;
      state.custom.touch.contacts = [];
    },
    update(state) {
      const snapshotJson = state.custom.emu.step_frame(
        Math.max(0, state.time.dtMs),
        JSON.stringify({
          viewport: {
            widthPx: state.profile.width,
            heightPx: state.profile.height,
          },
          gps: state.custom.gps,
          touch: state.custom.touch.contacts.length > 0 ? state.custom.touch : null,
        }),
      );
      state.custom.frame = JSON.parse(snapshotJson) as NonNullable<WasmRuntimeState["frame"]>;
      advanceTouchFrame(state.custom);
    },
    render(state, surface) {
      const ptr = state.custom.emu.pixels_ptr();
      const len = state.custom.emu.pixels_len();
      const bytes = new Uint8Array(wasm.memory.buffer, ptr, len);
      surface.pixels.set(bytes.subarray(0, surface.pixels.length));
    },
  };

  return {
    initialState: {
      emu,
      gps: null,
      touch: {
        sequence: 0,
        contacts: [],
      },
      frame: null,
    },
    program,
  };
}

function advanceTouchFrame(state: WasmRuntimeState): void {
  state.touch.contacts = state.touch.contacts
    .filter((contact) => contact.phase !== "ended" && contact.phase !== "cancelled")
    .map((contact) =>
      contact.phase === "started" || contact.phase === "moved"
        ? { ...contact, phase: "stationary" }
        : contact,
    );
}
