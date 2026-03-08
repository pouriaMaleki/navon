import type { Point } from "./types";

export class FrameBuffer {
  readonly width: number;
  readonly height: number;
  readonly pixels: Uint8Array;

  constructor(width: number, height: number) {
    this.width = width;
    this.height = height;
    this.pixels = new Uint8Array(width * height);
  }

  clear(value: number): void {
    this.pixels.fill(clampByte(value));
  }

  setPixel(x: number, y: number, value: number): void {
    if (x < 0 || y < 0 || x >= this.width || y >= this.height) {
      return;
    }
    const idx = y * this.width + x;
    const v = clampByte(value);
    if (v > this.pixels[idx]) {
      this.pixels[idx] = v;
    }
  }

  drawLine(a: Point, b: Point, value: number, radius: number): void {
    let x0 = Math.trunc(a.x);
    let y0 = Math.trunc(a.y);
    const x1 = Math.trunc(b.x);
    const y1 = Math.trunc(b.y);
    const dx = Math.abs(x1 - x0);
    const sx = x0 < x1 ? 1 : -1;
    const dy = -Math.abs(y1 - y0);
    const sy = y0 < y1 ? 1 : -1;
    let err = dx + dy;

    while (true) {
      this.stampCircle(x0, y0, radius, value);
      if (x0 === x1 && y0 === y1) {
        break;
      }
      const e2 = 2 * err;
      if (e2 >= dy) {
        err += dy;
        x0 += sx;
      }
      if (e2 <= dx) {
        err += dx;
        y0 += sy;
      }
    }
  }

  stampCircle(cx: number, cy: number, radius: number, value: number): void {
    const r = Math.max(1, Math.trunc(radius));
    for (let dy = -r; dy <= r; dy += 1) {
      for (let dx = -r; dx <= r; dx += 1) {
        if (dx * dx + dy * dy <= r * r) {
          this.setPixel(cx + dx, cy + dy, value);
        }
      }
    }
  }
}

function clampByte(v: number): number {
  if (v < 0) {
    return 0;
  }
  if (v > 255) {
    return 255;
  }
  return v | 0;
}
