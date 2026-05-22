import type { RoutingDiagEvent } from "../../../domain/routingDiagnosticsModels.js";

export const EVENT_KIND_LABELS: Record<string, string> = {
  locationUpdate: "GPS",
  audioCueDispatched: "Audio cue",
  nextTurnAlerted: "Turn alert",
  offRouteDetected: "Off route",
  rerouteRequested: "Reroute req",
  rerouteCompleted: "Reroute ok",
  routeStarted: "Route start",
  routeStopped: "Route stop",
  routeAlternativesSuggested: "Alternatives",
  routeSelected: "Route selected",
  compassModeChanged: "Compass",
  destinationChanged: "Dest changed",
  exploreAlternatives: "Explore",
};

export function eventSummary(e: RoutingDiagEvent): string {
  const d = e.data;
  switch (d.kind) {
    case "audioCueDispatched":
      return d.messageText;
    case "nextTurnAlerted":
      return d.instructionText;
    case "locationUpdate":
      return `${d.lat.toFixed(4)}, ${d.lon.toFixed(4)}`;
    case "offRouteDetected":
      return `${d.distanceM}m off route`;
    case "routeStopped":
      return d.reason ?? "stopped";
    case "rerouteCompleted":
      return d.result;
    case "routeAlternativesSuggested":
      return `${d.alternatives.length} options`;
    case "routeSelected":
      return d.label;
    case "compassModeChanged":
      return `${d.from} → ${d.to}`;
    default:
      return "";
  }
}
