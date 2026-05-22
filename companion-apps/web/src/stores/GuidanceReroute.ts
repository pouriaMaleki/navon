import {
  REROUTING_BACKOFF_DELAY_MS,
  REROUTING_BACKOFF_LONG_DELAY_MS,
  REROUTING_BACKOFF_WINDOW_MS,
  REROUTING_ESCALATE_AT_ATTEMPTS,
  REROUTING_THROTTLE_AT_ATTEMPTS,
} from "./GuidanceHelpers.js";

export function computeRerouteBackoff(
  attemptTimestamps: number[],
  now: number,
): { delayedUntilMs: number | undefined; recentAttempts: number } {
  const cutoff = now - REROUTING_BACKOFF_WINDOW_MS;
  const pruned = attemptTimestamps.filter((t) => t >= cutoff);
  const recent = pruned.length;
  let delayedUntilMs: number | undefined;
  if (recent >= REROUTING_ESCALATE_AT_ATTEMPTS) {
    delayedUntilMs = now + REROUTING_BACKOFF_LONG_DELAY_MS;
  } else if (recent >= REROUTING_THROTTLE_AT_ATTEMPTS) {
    delayedUntilMs = now + REROUTING_BACKOFF_DELAY_MS;
  }
  return { delayedUntilMs, recentAttempts: recent };
}

export function isWaitingToReroute(delayedUntilMs: number | undefined, now: number): boolean {
  return delayedUntilMs !== undefined && now < delayedUntilMs;
}
