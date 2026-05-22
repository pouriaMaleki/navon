import { type IReactionDisposer, reaction } from "mobx";
import { LOCATION_EVENT_THROTTLE_MS } from "../domain/routingDiagnosticsModels.js";
import type { RoutingDiagnosticsStore } from "../stores/RoutingDiagnosticsStore.js";
import type { RootStore } from "./RootStore.js";

const AUTO_STOP_AFTER_ARRIVAL_MS = 5_000;

/**
 * Installs MobX reactions that record routing diagnostics events.
 * Separate from the store so the store stays pure data+persistence.
 */
export function installRoutingDiagnosticsHooks(
  store: RootStore,
  diagStore: RoutingDiagnosticsStore,
): IReactionDisposer[] {
  const disposers: IReactionDisposer[] = [];

  // GPS location updates — throttled to avoid flooding
  let lastLocationEventMs = 0;
  disposers.push(
    reaction(
      () => store.locationStore.currentLocation,
      (location) => {
        if (!location || !diagStore.isRecording) return;
        const now = Date.now();
        if (now - lastLocationEventMs < LOCATION_EVENT_THROTTLE_MS) return;
        lastLocationEventMs = now;
        diagStore.recordEvent({
          kind: "locationUpdate",
          lat: location.latitude,
          lon: location.longitude,
          heading: store.locationStore.travelHeadingDegrees,
          speed: store.locationStore.currentSpeedMps,
          accuracyM: undefined,
        });
      },
      { fireImmediately: false },
    ),
  );

  // Destination changes
  let lastDestKey: string | null = null;
  disposers.push(
    reaction(
      () => {
        const s = store.guidanceStore.activeSession;
        if (!s.destinationLabel || !s.destinationCoordinate) return null;
        return {
          label: s.destinationLabel,
          lat: s.destinationCoordinate.latitude,
          lon: s.destinationCoordinate.longitude,
        };
      },
      (dest) => {
        if (!dest || !diagStore.isRecording) return;
        const key = `${dest.label}|${dest.lat}|${dest.lon}`;
        if (key === lastDestKey) return;
        lastDestKey = key;
        diagStore.recordEvent({
          kind: "destinationChanged",
          label: dest.label,
          lat: dest.lat,
          lon: dest.lon,
        });
      },
      { fireImmediately: false },
    ),
  );

  // Route alternatives suggested
  let lastAltsKey: string | null = null;
  disposers.push(
    reaction(
      () => {
        const alts = store.planningStore.preview.alternatives;
        if (alts.length === 0) return null;
        return alts.map((a) => ({
          providerName: a.normalizedPackage.provenance.providerID,
          routeId: a.normalizedPackage.routeIdentifier,
          label: a.title,
        }));
      },
      (alts) => {
        if (!alts || !diagStore.isRecording) return;
        const key = alts.map((a) => a.routeId).join(",");
        if (key === lastAltsKey) return;
        lastAltsKey = key;
        diagStore.recordEvent({
          kind: "routeAlternativesSuggested",
          alternatives: alts,
        });
      },
      { fireImmediately: false },
    ),
  );

  // Route started (homeMode -> phoneGuidance)
  disposers.push(
    reaction(
      () => store.guidanceStore.homeMode,
      (mode) => {
        if (mode !== "phoneGuidance" || !diagStore.isRecording) return;
        diagStore.recordEvent({ kind: "routeStarted" });
        // Also record route selection at the same time
        const preview = store.planningStore.preview;
        const selectedId = preview.selectedAlternativeID;
        if (selectedId) {
          const alt = preview.alternatives.find((a) => a.id === selectedId);
          if (alt) {
            diagStore.recordEvent({
              kind: "routeSelected",
              alternativeId: alt.id,
              providerName: alt.normalizedPackage.provenance.providerID,
              routeId: alt.normalizedPackage.routeIdentifier,
              label: alt.title,
            });
            // Record route geometry
            const route = store.guidanceStore.guidanceRoute;
            if (route?.geometry && route.geometry.length > 0) {
              diagStore.recordRouteGeometry({
                routeId: alt.normalizedPackage.routeIdentifier,
                providerName: alt.normalizedPackage.provenance.providerID,
                geometry: route.geometry,
              });
            }
          }
        }
      },
      { fireImmediately: false },
    ),
  );

  // Auto-stop recording after arrival
  let autoStopTimerId: ReturnType<typeof setTimeout> | undefined;
  disposers.push(
    reaction(
      () => store.guidanceStore.arrivalNotice,
      (notice) => {
        if (!notice || !diagStore.isRecording) return;
        if (autoStopTimerId) clearTimeout(autoStopTimerId);
        autoStopTimerId = setTimeout(() => {
          autoStopTimerId = undefined;
          diagStore.stopRecording();
        }, AUTO_STOP_AFTER_ARRIVAL_MS);
      },
      { fireImmediately: false },
    ),
  );

  // Route stopped (homeMode -> planning)
  disposers.push(
    reaction(
      () => store.guidanceStore.homeMode,
      (mode, prev) => {
        if (mode !== "planning" || prev !== "phoneGuidance" || !diagStore.isRecording) return;
        diagStore.recordEvent({
          kind: "routeStopped",
          reason: store.guidanceStore.arrivalNotice ?? undefined,
        });
        // Auto-stop diagnostics recording on manual stop (stop button).
        // Arrival auto-stop is handled separately by the arrivalNotice reaction.
        if (!store.guidanceStore.arrivalNotice) {
          if (autoStopTimerId) clearTimeout(autoStopTimerId);
          autoStopTimerId = setTimeout(() => {
            autoStopTimerId = undefined;
            diagStore.stopRecording();
          }, AUTO_STOP_AFTER_ARRIVAL_MS);
        }
      },
      { fireImmediately: false },
    ),
  );

  // Explore alternatives (splits icon)
  disposers.push(
    reaction(
      () => store.guidanceStore.isExploringAlternativesFromGuidance,
      (exploring) => {
        if (!exploring || !diagStore.isRecording) return;
        diagStore.recordEvent({ kind: "exploreAlternatives" });
      },
      { fireImmediately: false },
    ),
  );

  // Compass mode changes
  disposers.push(
    reaction(
      () => store.guidanceStore.compassMode,
      (to, from) => {
        if (!diagStore.isRecording) return;
        diagStore.recordEvent({
          kind: "compassModeChanged",
          from,
          to,
        });
      },
      { fireImmediately: false },
    ),
  );

  // Next turn alerts
  disposers.push(
    reaction(
      () => store.guidanceStore.upcomingTurnAlert,
      (alert) => {
        if (!alert || !diagStore.isRecording) return;
        diagStore.recordEvent({
          kind: "nextTurnAlerted",
          instructionText: alert.instructionText ?? alert.kind,
          distanceRemainingM: alert.distanceRemainingM,
        });
      },
      { fireImmediately: false },
    ),
  );

  // Off-route detection
  disposers.push(
    reaction(
      () => store.guidanceStore.offRoute,
      (offRoute) => {
        if (!offRoute || !diagStore.isRecording) return;
        diagStore.recordEvent({
          kind: "offRouteDetected",
          distanceM: store.guidanceStore.offRouteDistanceM,
        });
      },
      { fireImmediately: false },
    ),
  );

  // Reroute requested
  disposers.push(
    reaction(
      () => store.guidanceStore.rerouteRequested,
      (requested) => {
        if (!requested || !diagStore.isRecording) return;
        diagStore.recordEvent({ kind: "rerouteRequested" });
      },
      { fireImmediately: false },
    ),
  );

  return disposers;
}

/** Called directly (not via reaction) for events tied to async outcomes. */
export function recordRerouteCompleted(
  diagStore: RoutingDiagnosticsStore,
  result: "success" | "failed",
): void {
  if (!diagStore.isRecording) return;
  diagStore.recordEvent({ kind: "rerouteCompleted", result });
}

/** Called by the routing activity coordinator when an audio cue is dispatched. */
export function recordAudioCue(
  diagStore: RoutingDiagnosticsStore,
  cueType: string,
  messageText: string,
): void {
  if (!diagStore.isRecording) return;
  diagStore.recordEvent({ kind: "audioCueDispatched", cueType, messageText });
}
