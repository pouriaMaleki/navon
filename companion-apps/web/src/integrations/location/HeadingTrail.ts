import type { CoordinatePoint } from "../../domain/models.js";

/**
 * A tiny ring buffer of recent GPS fixes that derives a smoothed travel
 * heading. Semantics match runtime-core's motion filter: accumulate
 * displacement until it exceeds a minimum threshold, then EMA-smooth the
 * resulting bearing. Spec line 110 (authoritative): "camera rotates so
 * that riding direction is towards top of the screen this overrides the
 * camera of routing. Most important camera behaviour is this. (it needs
 * to determine the direction by last few GPS locations it receives)".
 */
export type HeadingTrailConfig = {
  /** Discard fixes older than this before computing heading. */
  maxAgeMs: number;
  /** Upper bound on buffered fixes. */
  maxFixes: number;
  /** Total displacement (oldest-to-newest) below this is treated as jitter. */
  minDisplacementM: number;
  /** Exponential smoothing factor for the output bearing. 0 = never change, 1 = raw. */
  smoothingAlpha: number;
};

type Fix = { point: CoordinatePoint; timestampMs: number };

export class HeadingTrail {
  private fixes: Fix[] = [];
  private smoothedDegrees: number | undefined;
  private config: HeadingTrailConfig;

  constructor(config: HeadingTrailConfig) {
    this.config = config;
  }

  recordFix(point: CoordinatePoint, timestampMs: number): void {
    this.evictOld(timestampMs);
    this.fixes.push({ point, timestampMs });
    if (this.fixes.length > this.config.maxFixes) this.fixes.shift();
    const raw = this.computeRawHeading();
    if (raw === undefined) return;
    const prev = this.smoothedDegrees;
    if (prev === undefined) {
      this.smoothedDegrees = raw;
      return;
    }
    // Smooth along the shortest arc so the EMA never goes "the long way".
    const delta = shortestSignedDelta(prev, raw);
    this.smoothedDegrees = normalize360(prev + delta * this.config.smoothingAlpha);
  }

  /** Current smoothed travel heading in degrees clockwise from north, or undefined. */
  get travelHeadingDegrees(): number | undefined {
    return this.smoothedDegrees;
  }

  reset(): void {
    this.fixes = [];
    this.smoothedDegrees = undefined;
  }

  private evictOld(nowMs: number): void {
    const cutoff = nowMs - this.config.maxAgeMs;
    while (this.fixes.length > 0 && this.fixes[0].timestampMs < cutoff) this.fixes.shift();
    // If everything aged out, the old smoothed heading is no longer
    // evidence-backed; drop it so a fresh leg's raw bearing primes from
    // scratch instead of EMA-ing toward it.
    if (this.fixes.length === 0) this.smoothedDegrees = undefined;
  }

  private computeRawHeading(): number | undefined {
    if (this.fixes.length < 2) return undefined;
    const first = this.fixes[0].point;
    const last = this.fixes[this.fixes.length - 1].point;
    const [dxEast, dyNorth] = eastNorthMeters(first, last);
    const displacement = Math.sqrt(dxEast * dxEast + dyNorth * dyNorth);
    if (displacement < this.config.minDisplacementM) return undefined;
    // Bearing from north, clockwise: atan2(east, north) in degrees.
    return normalize360((Math.atan2(dxEast, dyNorth) * 180.0) / Math.PI);
  }
}

function eastNorthMeters(a: CoordinatePoint, b: CoordinatePoint): [number, number] {
  const metersPerDegreeLat = 111_320.0;
  const meanLat = ((a.latitude + b.latitude) / 2.0) * (Math.PI / 180.0);
  const dNorth = (b.latitude - a.latitude) * metersPerDegreeLat;
  const dEast = (b.longitude - a.longitude) * Math.cos(meanLat) * metersPerDegreeLat;
  return [dEast, dNorth];
}

function normalize360(deg: number): number {
  const r = deg % 360.0;
  return r < 0 ? r + 360.0 : r;
}

function shortestSignedDelta(fromDeg: number, toDeg: number): number {
  let d = ((toDeg - fromDeg + 540.0) % 360.0) - 180.0;
  if (d === -180.0) d = 180.0;
  return d;
}
