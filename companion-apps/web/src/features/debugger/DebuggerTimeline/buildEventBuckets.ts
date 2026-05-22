import type { RoutingDiagEvent } from "../../../domain/routingDiagnosticsModels.js";

export interface EventBucket {
  startFraction: number;
  endFraction: number;
  events: RoutingDiagEvent[];
}

export function buildEventBuckets(
  events: RoutingDiagEvent[],
  startTimeMs: number,
  durationMs: number,
  bucketCount: number,
): EventBucket[] {
  const buckets: EventBucket[] = [];
  const bucketWidth = durationMs / bucketCount;
  for (let i = 0; i < bucketCount; i++) {
    buckets.push({
      startFraction: i / bucketCount,
      endFraction: (i + 1) / bucketCount,
      events: [],
    });
  }
  for (const e of events) {
    const idx = Math.min(bucketCount - 1, Math.floor((e.timestampMs - startTimeMs) / bucketWidth));
    if (idx >= 0) {
      buckets[idx].events.push(e);
    }
  }
  return buckets;
}
