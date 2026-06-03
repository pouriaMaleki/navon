import Foundation

/// Stateless display helpers — all inputs are explicit parameters, no side effects.
enum HomeDisplay {

    struct RoutingTopLayout: Equatable {
        let headline: String
        let subtitle: String
        let distanceToDestinationLine: String
        let minutesRemainingLine: String
        let offRouteLabel: String?
    }

    enum TopRightIcon: Equatable {
        case settings
        case compass
        case deviceChip
    }

    enum TopLeftIcon: Equatable {
        case alternateRoutes
        case zoomIn
        case zoomOut
    }

    static func shouldShowSearchPanel(
        homeMode: HomeMode,
        isSearchOpen: Bool,
        isResolvingUrl: Bool,
        urlResolveError: String?,
        query: String,
        hasRecentItems: Bool,
        hasVisibleSuggestions: Bool
    ) -> Bool {
        guard homeMode == .planning, isSearchOpen else { return false }
        if isResolvingUrl || urlResolveError != nil { return true }
        let trimmedQuery = query.trimmingCharacters(in: .whitespacesAndNewlines)
        if trimmedQuery.isEmpty {
            return hasRecentItems
        }
        return hasVisibleSuggestions
    }

    static func shouldShowSourceControl(
        homeMode: HomeMode,
        hasPreviewAlternatives: Bool,
        isPreviewLockedToImportedRoute: Bool,
        sourceModeOptionsCount: Int
    ) -> Bool {
        homeMode == .planning
            && hasPreviewAlternatives
            && !isPreviewLockedToImportedRoute
            && sourceModeOptionsCount > 1
    }

    static func routeSuggestionsTitle(isPreviewLockedToImportedRoute: Bool) -> String {
        isPreviewLockedToImportedRoute
            ? T.string("home.importedRoute")
            : T.string("home.suggestedRoutes")
    }

    static func isShowingActiveNavigation(homeMode: HomeMode) -> Bool {
        homeMode == .phoneGuidance || homeMode == .deviceOverview || homeMode == .sendingToDevice
    }

    static func startButtonTitle(homeMode: HomeMode, isDeviceConnected: Bool) -> String {
        switch homeMode {
        case .sendingToDevice:
            return T.string("home.startingOnDevice")
        case .planning:
            return isDeviceConnected
                ? T.string("home.startOnDevice")
                : T.string("home.start")
        case .phoneGuidance, .deviceOverview:
            return T.string("home.start")
        }
    }

    static func activeNavigationTitle(
        guidanceRouteDestinationLabel: String?,
        sessionDestinationLabel: String
    ) -> String {
        let placeholders: Set<String> = ["selected destination", "current location"]
        if let destination = guidanceRouteDestinationLabel {
            let trimmed = destination.trimmingCharacters(in: .whitespacesAndNewlines)
            if !trimmed.isEmpty && !placeholders.contains(trimmed.lowercased()) {
                return trimmed
            }
        }
        return sessionDestinationLabel
    }

    static func activeNavigationSubtitle(
        homeMode: HomeMode,
        remainingDistanceM: Double,
        remainingDurationSeconds: Double,
        guidanceRouteTotalDistanceMeters: Double,
        guidanceRouteEstimatedDurationSeconds: Int,
        lastSyncResult: String,
        selectedPreviewSummaryLine: String
    ) -> String {
        switch homeMode {
        case .phoneGuidance:
            let remaining = remainingDistanceM
            if remaining > 0 {
                let km = remaining / 1000
                let etaMin = max(1, Int(ceil(remainingDurationSeconds / 60.0)))
                return T.string("home.remainingKm", ["km": .number((km * 10).rounded() / 10), "min": .number(Double(etaMin))])
            }
            if guidanceRouteTotalDistanceMeters > 0 {
                let km = guidanceRouteTotalDistanceMeters / 1000
                let min = max(1, guidanceRouteEstimatedDurationSeconds / 60)
                return T.string("home.remainingKmShort", ["km": .number((km * 10).rounded() / 10), "min": .number(Double(min))])
            }
            return T.string("home.phoneGuidanceReady")
        case .deviceOverview, .sendingToDevice:
            return lastSyncResult
        case .planning:
            return selectedPreviewSummaryLine
        }
    }

    static func guidanceSubtitleLine(
        guidanceRouteDestinationLabel: String?,
        sessionDestinationLabel: String,
        activeNavigationSubtitle: String
    ) -> String {
        let raw = guidanceRouteDestinationLabel ?? sessionDestinationLabel
        let destination = raw.trimmingCharacters(in: .whitespacesAndNewlines)
        if destination.isEmpty || destination == "No destination" {
            return activeNavigationSubtitle
        }
        return "\(destination) • \(activeNavigationSubtitle)"
    }

    static func remainingDistanceM(routeTotalDistanceM: Double, progressDistanceM: Double) -> Double {
        guard routeTotalDistanceM > 0 else { return 0 }
        return max(0, routeTotalDistanceM - progressDistanceM)
    }

    static func remainingDurationSeconds(
        guidanceRouteTotalDistanceMeters: Double,
        guidanceRouteEstimatedDurationSeconds: Int,
        routeTotalDistanceM: Double,
        progressDistanceM: Double
    ) -> Double {
        guard routeTotalDistanceM > 0 else { return 0 }
        let fraction = max(0, 1 - progressDistanceM / routeTotalDistanceM)
        return Double(guidanceRouteEstimatedDurationSeconds) * fraction
    }

    static func routeOverviewGeometry(
        guidanceRouteGeometry: [CoordinatePoint]?,
        progressDistanceM: Double
    ) -> [CoordinatePoint] {
        guard let geometry = guidanceRouteGeometry, !geometry.isEmpty else { return [] }
        let split = PolylineGeo.splitPolylineAtDistance(geometry, distance: progressDistanceM)
        if split.remaining.count >= 2 { return split.remaining }
        return geometry
    }

    static func nextInstructionLine(
        guidanceRoute: NormalizedRoutePackage?,
        progressDistanceM: Double
    ) -> String? {
        guard let route = guidanceRoute else { return nil }
        for m in collapseCloseManeuvers(filterGlitchClusters(route.maneuvers, geometry: route.geometry), geometry: route.geometry) {
            if m.maneuverType == .depart || m.maneuverType == .arrive { continue }
            let remaining = m.distanceFromStartMeters - progressDistanceM
            if remaining < 0 { continue }
            let instruction = m.instructionText ?? T.string("maneuver.continue")
            return "\(PolylineGeo.formatDistance(remaining)) \(instruction)"
        }
        if let arrive = route.maneuvers.last(where: { $0.maneuverType == .arrive }) {
            let remaining = max(0, arrive.distanceFromStartMeters - progressDistanceM)
            return "\(PolylineGeo.formatDistance(remaining)) \(T.string("maneuver.arrive"))"
        }
        return nil
    }

    static func offRouteLabel(rerouteRequested: Bool, offRoute: Bool) -> String? {
        if rerouteRequested { return T.string("home.rerouting") }
        if offRoute { return T.string("home.offRoute") }
        return nil
    }

    static func topRightIconStack(isPaired: Bool) -> [TopRightIcon] {
        var icons: [TopRightIcon] = [.settings, .compass]
        if isPaired {
            icons.append(.deviceChip)
        }
        return icons
    }

    static func topLeftIconStack(homeMode: HomeMode) -> [TopLeftIcon] {
        var icons: [TopLeftIcon] = [.zoomIn, .zoomOut]
        if homeMode == .phoneGuidance {
            icons.append(.alternateRoutes)
        }
        return icons
    }

    static func compassSymbolName(compassMode: HomeCompassMode) -> String {
        switch compassMode {
        case .autoFollow:
            return "location.fill"
        case .northPreview:
            return "location.north.line.fill"
        case .northLocked:
            return "location.north.line.fill"
        }
    }

    static func selectedAlternativeIDForDisplay(
        isExploringAlternativesFromGuidance: Bool,
        explorationSelectedID: UUID?,
        previewSelectedAlternativeID: UUID?
    ) -> UUID? {
        isExploringAlternativesFromGuidance ? explorationSelectedID : previewSelectedAlternativeID
    }

    static func guidanceAlternatives(
        isExploringAlternativesFromGuidance: Bool,
        previewAlternatives: [RouteAlternative]
    ) -> [RouteAlternative] {
        isExploringAlternativesFromGuidance ? previewAlternatives : []
    }

    static func routingTopLayout(
        homeMode: HomeMode,
        nextInstructionLine: String?,
        activeNavigationTitle: String,
        offRouteLabel: String?,
        remainingDistanceM: Double,
        remainingDurationSeconds: Double,
        guidanceRoute: NormalizedRoutePackage?,
        sessionDestinationLabel: String
    ) -> RoutingTopLayout? {
        guard homeMode == .phoneGuidance else { return nil }
        let distanceLine = formattedDistanceToDestinationLine(
            remainingDistanceM: remainingDistanceM,
            guidanceRouteTotalDistanceMeters: guidanceRoute?.summary.totalDistanceMeters ?? 0,
            sessionDestinationLabel: sessionDestinationLabel,
            guidanceRouteDestinationLabel: guidanceRoute?.summary.destinationLabel
        )
        let minutesLine = formattedMinutesRemainingLine(
            remainingDurationSeconds: remainingDurationSeconds,
            guidanceRoute: guidanceRoute
        )
        let combined = [distanceLine, minutesLine].filter { !$0.isEmpty }.joined(separator: " • ")
        return RoutingTopLayout(
            headline: nextInstructionLine ?? activeNavigationTitle,
            subtitle: combined,
            distanceToDestinationLine: distanceLine,
            minutesRemainingLine: minutesLine,
            offRouteLabel: offRouteLabel
        )
    }

    static func formattedDistanceToDestinationLine(
        remainingDistanceM: Double,
        guidanceRouteTotalDistanceMeters: Double,
        sessionDestinationLabel: String,
        guidanceRouteDestinationLabel: String?
    ) -> String {
        let remaining = remainingDistanceM > 0 ? remainingDistanceM : guidanceRouteTotalDistanceMeters
        guard remaining > 0 else { return "" }
        let km = (remaining / 1000 * 10).rounded() / 10
        let candidates = [
            sessionDestinationLabel,
            guidanceRouteDestinationLabel ?? "",
        ]
        let placeholderTitles: Set<String> = [
            "", "No destination", "Selected destination", "Current location",
        ]
        let address = candidates
            .map { $0.trimmingCharacters(in: .whitespacesAndNewlines) }
            .first(where: { !placeholderTitles.contains($0) })
        if let address, !address.isEmpty {
            return T.string("home.distanceToDestination", ["km": .number(km), "destination": .string(address)])
        }
        return T.string("home.distanceKm", ["km": .number(km)])
    }

    static func formattedMinutesRemainingLine(
        remainingDurationSeconds: Double,
        guidanceRoute: NormalizedRoutePackage?
    ) -> String {
        let seconds: Double = remainingDurationSeconds > 0
            ? remainingDurationSeconds
            : Double(guidanceRoute?.summary.estimatedDurationSeconds ?? 0)
        guard seconds > 0 else { return "" }
        let minutes = max(1, Int(ceil(seconds / 60.0)))
        return T.string("home.minutesRemaining", ["min": .number(Double(minutes))])
    }
}
