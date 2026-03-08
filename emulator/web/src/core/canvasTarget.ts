import { FrameBuffer } from "./framebuffer";

export class CanvasTarget {
  private readonly ctx: CanvasRenderingContext2D;
  private readonly imageData: ImageData;
  private readonly rgba: Uint8ClampedArray;

  constructor(private readonly canvas: HTMLCanvasElement, width: number, height: number) {
    this.canvas.width = width;
    this.canvas.height = height;
    const ctx = canvas.getContext("2d");
    if (!ctx) {
      throw new Error("Could not create 2D context");
    }
    this.ctx = ctx;
    this.imageData = ctx.createImageData(width, height);
    this.rgba = this.imageData.data;
  }

  present(buffer: FrameBuffer): void {
    if (buffer.width !== this.canvas.width || buffer.height !== this.canvas.height) {
      throw new Error("Framebuffer and canvas resolution mismatch");
    }

    let j = 0;
    for (let i = 0; i < buffer.pixels.length; i += 1) {
      const v = buffer.pixels[i];
      this.rgba[j] = v;
      this.rgba[j + 1] = v;
      this.rgba[j + 2] = v;
      this.rgba[j + 3] = 255;
      j += 4;
    }
    this.ctx.putImageData(this.imageData, 0, 0);
  }
}
