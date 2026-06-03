import Foundation

/// Builds a `CueSnapshot` from routing state and dispatches to routing-activity and live-activity coordinators.
@MainActor
enum CueDispatcher {

    static func dispatch(
        isExploringAlternativesFromGuidance: Bool,
        guidanceRoute: NormalizedRoutePackage?,
        routeId: String?,
        progressDistanceM: Double,
        routeTotalDistanceM: Double,
        offRoute: Bool,
        rerouteRequested: Bool,
        arrivalNotice: String?,
        offRouteDistanceM: Double,
        isDeviceConnectedForCueSuppression: Bool,
        routingActivityCoordinator: RoutingActivityCoordinator,
        liveActivityCoordinator: LiveActivityCoordinator,
        isRoutingInProgress: Bool,
        isAppInBackground: Bool,
        settings: CompanionSettings,
        onSetActiveGuidanceRoute: (NormalizedRoutePackage?) -> Void
    ) {
        guard !isExploringAlternativesFromGuidance else { return }
        let pairedWithDevice = isDeviceConnectedForCueSuppression
        let filteredManeuvers = guidanceRoute.map { collapseCloseManeuvers(filterGlitchClusters($0.maneuvers, geometry: $0.geometry), geometry: $0.geometry) } ?? []
        let cueManeuvers: [CueManeuver] = filteredManeuvers.compactMap { m in
            CueManeuverMapping.kind(for: m.maneuverType).map { kind in
                CueManeuver(
                    id: m.id,
                    kind: kind,
                    distanceFromStartM: m.distanceFromStartMeters
                )
            }
        }
        let snapshot = CueSnapshot(
            routeId: routeId,
            pairedWithDevice: pairedWithDevice,
            progressDistanceM: progressDistanceM,
            maneuvers: cueManeuvers,
            offRoute: offRoute,
            rerouting: rerouteRequested,
            arrived: arrivalNotice != nil,
            distanceFromRouteM: offRouteDistanceM,
            routeTotalDistanceM: routeTotalDistanceM
        )
        routingActivityCoordinator.onGuidanceTick(
            snapshot: snapshot,
            settings: settings,
            isRouting: isRoutingInProgress,
            isAppInBackground: isAppInBackground
        )
        onSetActiveGuidanceRoute(guidanceRoute)
        let isImperial = T.resolveDistanceUnit(settings.distanceUnit) == .imperial
        liveActivityCoordinator.onGuidanceTick(
            settings: settings,
            isRouting: isRoutingInProgress,
            route: guidanceRoute,
            progressDistanceM: progressDistanceM,
            offRoute: offRoute,
            rerouting: rerouteRequested,
            arrived: arrivalNotice != nil,
            isImperial: isImperial
        )
    }
}
