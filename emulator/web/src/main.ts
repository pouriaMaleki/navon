import { Esp32ScreenEmulator } from "./core/emulator";
import { WAVESHARE_ESP32_P4_3_4 } from "./core/screenProfiles";
import { createWasmMinimapProgram } from "./programs/wasmMinimapProgram";

async function bootstrap(): Promise<void> {
  const canvas = document.getElementById("minimap");
  const toggleBtn = document.getElementById("toggle");
  const resetBtn = document.getElementById("reset");

  if (!(canvas instanceof HTMLCanvasElement)) {
    throw new Error("Expected #minimap canvas element");
  }
  if (!(toggleBtn instanceof HTMLButtonElement) || !(resetBtn instanceof HTMLButtonElement)) {
    throw new Error("Expected emulator control buttons");
  }

  const { initialState, program } = await createWasmMinimapProgram(0);
  const emulator = new Esp32ScreenEmulator(canvas, WAVESHARE_ESP32_P4_3_4, initialState, program);
  emulator.renderOnce();
  emulator.start();

  toggleBtn.addEventListener("click", () => {
    const running = emulator.toggle();
    toggleBtn.textContent = running ? "Pause" : "Resume";
  });

  resetBtn.addEventListener("click", () => {
    emulator.reset();
  });
}

void bootstrap();
