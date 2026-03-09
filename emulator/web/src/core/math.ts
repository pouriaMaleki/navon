import type { Point, WorldBounds } from "./types";

export function worldToScreen(
  point: Point,
  bounds: WorldBounds,
  width: number,
  height: number,
): Point {
  const bw = Math.max(1, bounds.maxX - bounds.minX);
  const bh = Math.max(1, bounds.maxY - bounds.minY);
  const px = point.x - bounds.minX;
  const py = point.y - bounds.minY;
  const x = Math.floor((px * (width - 1)) / bw);
  const y = Math.floor(((bh - py) * (height - 1)) / bh);
  return { x, y };
}
