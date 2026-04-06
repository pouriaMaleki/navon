import Foundation

struct SampleRoutingAdapter: RoutingProvider {
    let providerID: RouteProviderID
    let isAvailableInV1: Bool = false

    func planRoute(_ request: RoutePlanRequest) async throws -> RoutePreviewModel {
        buildPreview(for: request, revision: 1, planningNotice: planningNotice(for: providerID))
    }

    func replanRoute(using session: ActiveRouteSession, riderLocation: CoordinatePoint) async throws -> RoutePreviewModel {
        let request = RoutePlanRequest(
            origin: riderLocation,
            destination: session.destinationCoordinate ?? riderLocation,
            providerID: providerID
        )
        return buildPreview(for: request, revision: (session.routeRevision ?? 0) + 1, planningNotice: "Rerouted with \(providerID.displayName) sample adapter")
    }

    func normalizePreview(_ preview: RoutePreviewModel, request: RoutePlanRequest) throws -> NormalizedRoutePackage {
        guard let selected = preview.selectedAlternative else {
            throw SampleRoutingAdapterError.noAlternativesAvailable
        }
        return selected.normalizedPackage
    }

    private func buildPreview(for request: RoutePlanRequest, revision: Int, planningNotice: String) -> RoutePreviewModel {
        let alternatives = [0, 1, 2].map { alternativeIndex in
            buildAlternative(for: request, revision: revision, alternativeIndex: alternativeIndex)
        }
        return RoutePreviewModel(
            alternatives: alternatives,
            selectedAlternativeID: alternatives.first?.id,
            routeIdentifier: alternatives.first?.normalizedPackage.routeIdentifier,
            routeRevision: alternatives.first?.normalizedPackage.revision,
            planningNotice: planningNotice
        )
    }

    private func buildAlternative(for request: RoutePlanRequest, revision: Int, alternativeIndex: Int) -> RouteAlternative {
        let geometry = sampleGeometry(from: request.origin, to: request.destination, alternativeIndex: alternativeIndex)
        let maneuvers = buildManeuvers(geometry: geometry)
        let totalDistance = routeDistance(for: geometry)
        let routeID = "\(providerID.rawValue)-sample-\(alternativeIndex)"
        let summary = RouteSummary(
            totalDistanceMeters: totalDistance,
            estimatedDurationSeconds: max(Int((totalDistance / providerAverageMetersPerSecond(providerID)).rounded()), 60),
            startLabel: "Current location",
            destinationLabel: "\(providerID.displayName) sample destination"
        )
        let package = NormalizedRoutePackage(
            version: .current,
            routeIdentifier: routeID,
            revision: revision,
            geometry: geometry,
            maneuvers: maneuvers,
            summary: summary,
            provenance: RouteProvenance(
                providerID: providerID,
                sourceReference: "\(providerID.displayName) sample adapter",
                generatedAtUnixMs: UInt64(Date().timeIntervalSince1970 * 1000)
            )
        )
        return RouteAlternative(
            id: UUID(),
            title: alternativeIndex == 0 ? "Sample primary route" : "Sample alternative route",
            subtitle: subtitle(for: providerID, alternativeIndex: alternativeIndex),
            distanceMeters: Int(totalDistance.rounded()),
            durationSeconds: summary.estimatedDurationSeconds,
            normalizedPackage: package
        )
    }

    private func sampleGeometry(from origin: CoordinatePoint, to destination: CoordinatePoint, alternativeIndex: Int) -> [CoordinatePoint] {
        let latDelta = destination.latitude - origin.latitude
        let lonDelta = destination.longitude - origin.longitude
        let length = max(sqrt(latDelta * latDelta + lonDelta * lonDelta), 0.0001)
        let perpendicularLat = -lonDelta / length
        let perpendicularLon = latDelta / length
        let baseOffset = providerOffset(providerID) * Double(alternativeIndex == 0 ? 1 : -1)

        let first = CoordinatePoint(
            latitude: origin.latitude + latDelta * 0.28 + perpendicularLat * baseOffset,
            longitude: origin.longitude + lonDelta * 0.28 + perpendicularLon * baseOffset
        )
        let second = CoordinatePoint(
            latitude: origin.latitude + latDelta * 0.56 - perpendicularLat * baseOffset * 0.8,
            longitude: origin.longitude + lonDelta * 0.56 - perpendicularLon * baseOffset * 0.8
        )
        let third = CoordinatePoint(
            latitude: origin.latitude + latDelta * 0.82 + perpendicularLat * baseOffset * 0.35,
            longitude: origin.longitude + lonDelta * 0.82 + perpendicularLon * baseOffset * 0.35
        )

        var geometry = [origin, first, second, third, destination]
        if origin == destination {
            geometry = [
                origin,
                CoordinatePoint(latitude: origin.latitude + 0.002, longitude: origin.longitude + 0.0015),
                CoordinatePoint(latitude: origin.latitude + 0.003, longitude: origin.longitude - 0.001),
                CoordinatePoint(latitude: origin.latitude + 0.001, longitude: origin.longitude - 0.002),
                origin,
            ]
        }
        return deduplicated(geometry)
    }

    private func buildManeuvers(geometry: [CoordinatePoint]) -> [RouteManeuver] {
        let cumulative = cumulativeDistances(for: geometry)
        var maneuvers = [
            RouteManeuver(
                id: "depart",
                maneuverType: .depart,
                location: geometry.first ?? CoordinatePoint(latitude: 0, longitude: 0),
                distanceFromStartMeters: 0,
                distanceToNextMeters: cumulative.dropFirst().first,
                instructionText: "Start riding"
            )
        ]

        for index in 1..<(geometry.count - 1) {
            let delta = turnDeltaDegrees(previous: geometry[index - 1], current: geometry[index], next: geometry[index + 1])
            guard let maneuver = classifyTurn(deltaDegrees: delta) else { continue }
            let distanceToNext = index + 1 < cumulative.count ? cumulative[index + 1] - cumulative[index] : nil
            maneuvers.append(
                RouteManeuver(
                    id: "step-\(index)",
                    maneuverType: maneuver.type,
                    location: geometry[index],
                    distanceFromStartMeters: cumulative[index],
                    distanceToNextMeters: distanceToNext,
                    instructionText: maneuver.instruction
                )
            )
        }

        maneuvers.append(
            RouteManeuver(
                id: "arrive",
                maneuverType: .arrive,
                location: geometry.last ?? CoordinatePoint(latitude: 0, longitude: 0),
                distanceFromStartMeters: cumulative.last ?? 0,
                distanceToNextMeters: nil,
                instructionText: "Arrive at destination"
            )
        )
        return maneuvers
    }

    private func cumulativeDistances(for geometry: [CoordinatePoint]) -> [Double] {
        var cumulative = [0.0]
        for (start, end) in zip(geometry, geometry.dropFirst()) {
            cumulative.append(cumulative.last! + approximateDistanceMeters(from: start, to: end))
        }
        return cumulative
    }

    private func routeDistance(for geometry: [CoordinatePoint]) -> Double {
        zip(geometry, geometry.dropFirst()).reduce(0.0) { partial, segment in
            partial + approximateDistanceMeters(from: segment.0, to: segment.1)
        }
    }

    private func approximateDistanceMeters(from start: CoordinatePoint, to end: CoordinatePoint) -> Double {
        let latMeters = (end.latitude - start.latitude) * 111_320.0
        let lonMeters = (end.longitude - start.longitude) * cos(((start.latitude + end.latitude) / 2.0) * .pi / 180.0) * 111_320.0
        return sqrt(latMeters * latMeters + lonMeters * lonMeters)
    }

    private func turnDeltaDegrees(previous: CoordinatePoint, current: CoordinatePoint, next: CoordinatePoint) -> Double {
        let incoming = bearingDegrees(from: previous, to: current)
        let outgoing = bearingDegrees(from: current, to: next)
        var delta = outgoing - incoming
        while delta <= -180.0 { delta += 360.0 }
        while delta > 180.0 { delta -= 360.0 }
        return delta
    }

    private func bearingDegrees(from start: CoordinatePoint, to end: CoordinatePoint) -> Double {
        let latMeters = (end.latitude - start.latitude) * 111_320.0
        let lonMeters = (end.longitude - start.longitude) * cos(((start.latitude + end.latitude) / 2.0) * .pi / 180.0) * 111_320.0
        return atan2(lonMeters, latMeters) * 180.0 / .pi
    }

    private func classifyTurn(deltaDegrees: Double) -> (type: RouteManeuverType, instruction: String)? {
        let magnitude = abs(deltaDegrees)
        guard magnitude >= 25 else { return nil }
        if magnitude >= 170 { return (.uturn, "Make a U-turn") }
        if magnitude >= 110 { return (deltaDegrees > 0 ? .sharpRight : .sharpLeft, deltaDegrees > 0 ? "Turn sharply right" : "Turn sharply left") }
        if magnitude >= 50 { return (deltaDegrees > 0 ? .right : .left, deltaDegrees > 0 ? "Turn right" : "Turn left") }
        return (deltaDegrees > 0 ? .slightRight : .slightLeft, deltaDegrees > 0 ? "Bear right" : "Bear left")
    }

    private func deduplicated(_ points: [CoordinatePoint]) -> [CoordinatePoint] {
        var output: [CoordinatePoint] = []
        for point in points where output.last != point {
            output.append(point)
        }
        return output
    }

    private func planningNotice(for provider: RouteProviderID) -> String {
        switch provider {
        case .osm:
            return "Using sample OSM fallback routes. Live OSM routing is not wired yet."
        case .googleIngest:
            return "Using sample Google ingest routes. Compliance and live ingestion are still pending."
        case .gpxImport:
            return "Using sample GPX import routes. File selection is not wired yet."
        case .fitImport:
            return "Using sample FIT import routes. File selection is not wired yet."
        case .tcxImport:
            return "Using sample TCX import routes. File selection is not wired yet."
        case .garminApi:
            return "Using sample Garmin API routes. Live Garmin integration is not wired yet."
        case .garminFile:
            return "Using sample Garmin file routes. File selection is not wired yet."
        case .hsl:
            return "Using sample HSL route"
        }
    }

    private func subtitle(for provider: RouteProviderID, alternativeIndex: Int) -> String {
        let variant = alternativeIndex == 0 ? "primary" : "alternative"
        return "\(provider.displayName) sample \(variant)"
    }

    private func providerOffset(_ provider: RouteProviderID) -> Double {
        switch provider {
        case .osm:
            return 0.0020
        case .googleIngest:
            return 0.0018
        case .gpxImport:
            return 0.0014
        case .fitImport:
            return 0.0016
        case .tcxImport:
            return 0.0012
        case .garminApi:
            return 0.0017
        case .garminFile:
            return 0.0015
        case .hsl:
            return 0.0018
        }
    }

    private func providerAverageMetersPerSecond(_ provider: RouteProviderID) -> Double {
        switch provider {
        case .osm:
            return 5.2
        case .googleIngest:
            return 5.6
        case .gpxImport:
            return 4.8
        case .fitImport:
            return 5.0
        case .tcxImport:
            return 4.7
        case .garminApi:
            return 5.4
        case .garminFile:
            return 4.9
        case .hsl:
            return 5.3
        }
    }
}

enum SampleRoutingAdapterError: LocalizedError {
    case noAlternativesAvailable

    var errorDescription: String? {
        switch self {
        case .noAlternativesAvailable:
            return "No sample route alternatives are available"
        }
    }
}
