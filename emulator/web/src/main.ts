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
  const custom = emulator.customState();
  emulator.renderOnce();
  emulator.start();

  installGeo(custom);
  installTouchControls(canvas, custom);

  toggleBtn.addEventListener("click", () => {
    const running = emulator.toggle();
    toggleBtn.textContent = running ? "Pause" : "Resume";
  });

  resetBtn.addEventListener("click", () => {
    emulator.reset();
  });
}

void bootstrap();

function installGeo(custom: {
  hasGeo: boolean;
  lat: number;
  lon: number;
  headingRad: number;
}): void {
  if (!("geolocation" in navigator)) {
    return;
  }

  let prevLat = 0;
  let prevLon = 0;
  let hasPrev = false;

  navigator.geolocation.watchPosition(
    (pos) => {
      const lat = pos.coords.latitude;
      const lon = pos.coords.longitude;
      custom.hasGeo = true;
      custom.lat = lat;
      custom.lon = lon;

      let heading = pos.coords.heading;
      if ((heading === null || Number.isNaN(heading)) && hasPrev) {
        heading = bearingDeg(prevLat, prevLon, lat, lon);
      }
      if (heading !== null && !Number.isNaN(heading)) {
        custom.headingRad = (heading * Math.PI) / 180;
      }
      prevLat = lat;
      prevLon = lon;
      hasPrev = true;
    },
    () => {
      // Keep fallback simulation if location permission is denied.
    },
    { enableHighAccuracy: true, maximumAge: 1000, timeout: 10000 }
  );
}

function installTouchControls(
  canvas: HTMLCanvasElement,
  custom: {
    zoom: number;
    panX: number;
    panY: number;
    lastPanInputMs: number;
  }
): void {
  const pointers = new Map<number, { x: number; y: number }>();
  let lastPinchDistance = 0;

  const toLocal = (ev: PointerEvent): { x: number; y: number } => {
    const r = canvas.getBoundingClientRect();
    return { x: ev.clientX - r.left, y: ev.clientY - r.top };
  };

  const updatePan = (dx: number, dy: number): void => {
    const mapPerPixel = 10000 / Math.max(1, canvas.clientWidth * custom.zoom);
    custom.panX = clamp(custom.panX - dx * mapPerPixel, -4500, 4500);
    custom.panY = clamp(custom.panY + dy * mapPerPixel, -4500, 4500);
    custom.lastPanInputMs = performance.now();
  };

  canvas.addEventListener("pointerdown", (ev) => {
    canvas.setPointerCapture(ev.pointerId);
    pointers.set(ev.pointerId, toLocal(ev));
    if (pointers.size === 2) {
      const pts = [...pointers.values()];
      lastPinchDistance = distance(pts[0], pts[1]);
    }
  });

  canvas.addEventListener("pointermove", (ev) => {
    if (!pointers.has(ev.pointerId)) {
      return;
    }
    const prev = pointers.get(ev.pointerId)!;
    const cur = toLocal(ev);
    pointers.set(ev.pointerId, cur);

    if (pointers.size === 1 && ev.pressure > 0) {
      updatePan(cur.x - prev.x, cur.y - prev.y);
      return;
    }
    if (pointers.size >= 2) {
      const pts = [...pointers.values()];
      const d = distance(pts[0], pts[1]);
      if (lastPinchDistance > 0) {
        const ratio = d / lastPinchDistance;
        custom.zoom = clamp(custom.zoom * ratio, 0.6, 4.5);
        custom.lastPanInputMs = performance.now();
      }
      lastPinchDistance = d;
    }
  });

  const release = (ev: PointerEvent): void => {
    pointers.delete(ev.pointerId);
    if (pointers.size < 2) {
      lastPinchDistance = 0;
    }
  };
  canvas.addEventListener("pointerup", release);
  canvas.addEventListener("pointercancel", release);
}

function bearingDeg(lat1: number, lon1: number, lat2: number, lon2: number): number {
  const p1 = (lat1 * Math.PI) / 180;
  const p2 = (lat2 * Math.PI) / 180;
  const dLon = ((lon2 - lon1) * Math.PI) / 180;
  const y = Math.sin(dLon) * Math.cos(p2);
  const x = Math.cos(p1) * Math.sin(p2) - Math.sin(p1) * Math.cos(p2) * Math.cos(dLon);
  return (Math.atan2(y, x) * 180) / Math.PI;
}

function distance(a: { x: number; y: number }, b: { x: number; y: number }): number {
  const dx = a.x - b.x;
  const dy = a.y - b.y;
  return Math.hypot(dx, dy);
}

function clamp(v: number, min: number, max: number): number {
  return Math.max(min, Math.min(max, v));
}
