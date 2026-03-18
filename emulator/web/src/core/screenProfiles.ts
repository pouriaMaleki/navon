import type { ScreenProfile } from "./types";

export const WAVESHARE_ESP32_P4_3_4: ScreenProfile = {
  id: "waveshare-esp32-p4-3.4-800x800",
  label: "Waveshare ESP32-P4 3.4-inch",
  width: 800,
  height: 800,
};

export function browserViewportProfile(canvas?: HTMLCanvasElement): ScreenProfile {
  const rect = canvas?.getBoundingClientRect();
  const rectWidth = rect ? Math.round(rect.width) : 0;
  const rectHeight = rect ? Math.round(rect.height) : 0;
  const viewport = window.visualViewport;
  const width = Math.max(1, rectWidth || Math.round(viewport?.width ?? window.innerWidth));
  const height = Math.max(1, rectHeight || Math.round(viewport?.height ?? window.innerHeight));
  return {
    id: `browser-viewport-${width}x${height}`,
    label: "Browser viewport",
    width,
    height,
  };
}
