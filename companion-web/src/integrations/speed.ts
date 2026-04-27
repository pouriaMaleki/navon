import type { SpeedUnit } from "../domain/models.js";

const KPH_PER_MPS = 3.6;
const MPH_PER_MPS = 2.2369363;

export function mpsToUnit(mps: number, unit: SpeedUnit): number {
  const factor = unit === "mph" ? MPH_PER_MPS : KPH_PER_MPS;
  return Math.round(mps * factor);
}

export function formatSpeedLabel(mps: number | undefined, unit: SpeedUnit): string {
  const label = unit === "mph" ? "mph" : "km/h";
  if (mps === undefined || !Number.isFinite(mps)) return `— ${label}`;
  return `${mpsToUnit(mps, unit)} ${label}`;
}
