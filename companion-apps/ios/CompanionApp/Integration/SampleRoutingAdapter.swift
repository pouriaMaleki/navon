import Foundation

struct SampleRoutingAdapter: RoutingProvider {
    let providerID: RouteProviderID
    let isAvailableInV1: Bool = false

    func importFile(named fileName: String, data: Data, origin: CoordinatePoint, revision: Int = 1) throws -> RoutePreviewModel {
        guard !data.isEmpty else {
            throw SampleRoutingAdapterError.emptyImportFile
        }

        let request = RoutePlanRequest(
            origin: origin,
            destination: importedFileDestination(from: origin),
            providerID: providerID
        )
        return buildPreview(
            for: request,
            revision: revision,
            planningNotice: "Using sample \(providerID.displayName) preview for shared file \(fileName). Live file parsing is not wired yet."
        )
    }

    func planRoute(_ request: RoutePlanRequest) async throws -> RoutePreviewModel {
        if providerID == .osm {
            return try await buildLiveOsmPreview(for: request, revision: 1)
        }
        return buildPreview(for: request, revision: 1, planningNotice: planningNotice(for: providerID))
    }

    func replanRoute(
        using session: ActiveRouteSession,
        riderLocation: CoordinatePoint,
        rerouteContext: RerouteContext?
    ) async throws -> RoutePreviewModel {

        let request = RoutePlanRequest(
            origin: riderLocation,
            destination: session.destinationCoordinate ?? riderLocation,
            providerID: providerID
        )
        let revision = (session.routeRevision ?? 0) + 1
        if providerID == .osm {
            return try await buildLiveOsmPreview(for: request, revision: revision)
        }
        return buildPreview(
            for: request,
            revision: revision,
            planningNotice: "Rerouted with \(providerID.displayName) sample adapter"
        )
    }

    func normalizePreview(_ preview: RoutePreviewModel, request: RoutePlanRequest) throws -> NormalizedRoutePackage {
        guard let selected = preview.selectedAlternative else {
            throw SampleRoutingAdapterError.noAlternativesAvailable
        }
        return selected.normalizedPackage
    }

    private func buildLiveOsmPreview(for request: RoutePlanRequest, revision: Int) async throws -> RoutePreviewModel {
        do {
            return try await fetchLiveOsmPreview(for: request, revision: revision)
        } catch {
            return buildPreview(
                for: request,
                revision: revision,
                planningNotice: "Live OSM failed: \(displayMessage(for: error)). Showing sample route instead."
            )
        }
    }

    private func fetchLiveOsmPreview(for request: RoutePlanRequest, revision: Int) async throws -> RoutePreviewModel {
        let coordinates = String(
            format: "%.6f,%.6f;%.6f,%.6f",
            request.origin.longitude,
            request.origin.latitude,
            request.destination.longitude,
            request.destination.latitude
        )
        guard let url = URL(string: "https://router.project-osrm.org/route/v1/bike/\(coordinates)?alternatives=3&overview=full&steps=true&geometries=polyline") else {
            throw SampleRoutingAdapterError.invalidLiveRouteURL
        }

        let (data, response) = try await URLSession.shared.data(from: url)
        guard let http = response as? HTTPURLResponse else {
            throw SampleRoutingAdapterError.networkFailure("Missing HTTP response")
        }
        guard (200 ..< 300).contains(http.statusCode) else {
            let bodyMessage = String(data: data, encoding: .utf8) ?? "No response body"
            throw SampleRoutingAdapterError.networkFailure("HTTP \(http.statusCode): \(bodyMessage)")
        }

        let decoded = try JSONDecoder().decode(OSRMRouteResponse.self, from: data)
        guard decoded.code == "Ok" else {
            throw SampleRoutingAdapterError.networkFailure(decoded.message ?? "OSRM returned \(decoded.code)")
        }
        guard !decoded.routes.isEmpty else {
            throw SampleRoutingAdapterError.noAlternativesAvailable
        }

        let alternatives = decoded.routes.prefix(3).enumerated().compactMap { index, route in
            mapLiveAlternative(route: route, request: request, revision: revision, alternativeIndex: index)
        }
        guard !alternatives.isEmpty else {
            throw SampleRoutingAdapterError.noAlternativesAvailable
        }

        return RoutePreviewModel(
            alternatives: alternatives,
            selectedAlternativeID: alternatives.first?.id,
            routeIdentifier: alternatives.first?.normalizedPackage.routeIdentifier,
            routeRevision: alternatives.first?.normalizedPackage.revision,
            planningNotice: "Live OSM bike routing via OSRM demo server"
        )
    }

    private func mapLiveAlternative(
        route: OSRMRoute,
        request: RoutePlanRequest,
        revision: Int,
        alternativeIndex: Int
    ) -> RouteAlternative? {
        let geometry = ShareImportUtilities.decodePolyline(route.geometry)
        guard geometry.count >= 2 else { return nil }
        let maneuvers = buildLiveManeuvers(from: route, geometry: geometry)
        let summary = RouteSummary(
            totalDistanceMeters: route.distance,
            estimatedDurationSeconds: max(Int(route.duration.rounded()), 60),
            startLabel: "Current location",
            destinationLabel: "Selected destination"
        )
        let routeID = buildLiveRouteIdentifier(request: request, alternativeIndex: alternativeIndex)
        let package = NormalizedRoutePackage(
            version: .current,
            routeIdentifier: routeID,
            revision: revision,
            geometry: geometry,
            maneuvers: maneuvers,
            summary: summary,
            provenance: RouteProvenance(
                providerID: .osm,
                sourceReference: "OSRM bike route",
                generatedAtUnixMs: UInt64(Date().timeIntervalSince1970 * 1000)
            )
        )
        return RouteAlternative(
            id: UUID(),
            title: alternativeIndex == 0 ? "OSRM primary route" : "OSRM alternative route",
            subtitle: route.legs.first?.summary?.isEmpty == false ? route.legs.first?.summary ?? "OSRM bike route" : "OSRM bike route",
            distanceMeters: Int(route.distance.rounded()),
            durationSeconds: summary.estimatedDurationSeconds,
            normalizedPackage: package
        )
    }

    private func buildLiveManeuvers(from route: OSRMRoute, geometry: [CoordinatePoint]) -> [RouteManeuver] {
        var maneuvers: [RouteManeuver] = [
            RouteManeuver(
                id: "depart",
                maneuverType: .depart,
                location: geometry.first ?? CoordinatePoint(latitude: 0, longitude: 0),
                distanceFromStartMeters: 0,
                distanceToNextMeters: firstLiveStepDistance(in: route.legs),
                instructionText: "Start riding"
            )
        ]

        var distanceFromStart = 0.0
        for leg in route.legs {
            for step in leg.steps {
                defer { distanceFromStart += step.distance }
                let instruction = instructionText(for: step)
                let type = step.maneuver.type.lowercased()
                switch type {
                case "depart", "notification", "new name", "continue", "arrive":
                    continue
                default:
                    break
                }
                guard step.maneuver.location.count >= 2 else { continue }
                maneuvers.append(
                    RouteManeuver(
                        id: step.name.isEmpty ? "step-\(maneuvers.count)" : "step-\(maneuvers.count)-\(step.name)",
                        maneuverType: maneuverType(for: step),
                        location: CoordinatePoint(latitude: step.maneuver.location[1], longitude: step.maneuver.location[0]),
                        distanceFromStartMeters: distanceFromStart,
                        distanceToNextMeters: step.distance,
                        instructionText: instruction
                    )
                )
            }
        }

        maneuvers.append(
            RouteManeuver(
                id: "arrive",
                maneuverType: .arrive,
                location: geometry.last ?? CoordinatePoint(latitude: 0, longitude: 0),
                distanceFromStartMeters: route.distance,
                distanceToNextMeters: nil,
                instructionText: "Arrive at destination"
            )
        )
        return maneuvers
    }

    private func firstLiveStepDistance(in legs: [OSRMLeg]) -> Double? {
        for leg in legs {
            for step in leg.steps where step.maneuver.type.lowercased() != "depart" {
                return step.distance
            }
        }
        return nil
    }

    private func maneuverType(for step: OSRMStep) -> RouteManeuverType {
        let type = step.maneuver.type.lowercased()
        let modifier = step.maneuver.modifier?.lowercased() ?? ""
        switch type {
        case "roundabout", "rotary":
            return .roundabout
        case "merge", "fork", "on ramp", "off ramp":
            return .merge
        case "end of road", "turn", "continue", "use lane", "notification", "new name":
            break
        case "arrive":
            return .arrive
        default:
            break
        }

        switch modifier {
        case "uturn":
            return .uturn
        case "sharp right":
            return .sharpRight
        case "right":
            return .right
        case "slight right":
            return .slightRight
        case "sharp left":
            return .sharpLeft
        case "left":
            return .left
        case "slight left":
            return .slightLeft
        default:
            return .straight
        }
    }

    private func instructionText(for step: OSRMStep) -> String {
        let type = step.maneuver.type.lowercased()
        let modifier = step.maneuver.modifier?.lowercased() ?? ""
        switch type {
        case "roundabout", "rotary":
            return "Enter roundabout"
        case "arrive":
            return "Arrive at destination"
        case "merge":
            return "Merge"
        case "fork":
            return modifier.contains("left") ? "Keep left" : modifier.contains("right") ? "Keep right" : "Keep to the fork"
        default:
            break
        }

        switch modifier {
        case "uturn":
            return "Make a U-turn"
        case "sharp right":
            return "Turn sharply right"
        case "right":
            return "Turn right"
        case "slight right":
            return "Slight right"
        case "sharp left":
            return "Turn sharply left"
        case "left":
            return "Turn left"
        case "slight left":
            return "Slight left"
        default:
            return step.name.isEmpty ? "Continue" : "Continue on \(step.name)"
        }
    }

    private func buildLiveRouteIdentifier(request: RoutePlanRequest, alternativeIndex: Int) -> String {
        let origin = String(format: "%.5f,%.5f", request.origin.latitude, request.origin.longitude)
        let destination = String(format: "%.5f,%.5f", request.destination.latitude, request.destination.longitude)
        return "osm-live:\(origin)->\(destination):alt-\(alternativeIndex)"
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
        let totalDistance = PolylineGeo.polylineLengthMeters(geometry)
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
        if origin == destination {
            return deduplicated([
                origin,
                CoordinatePoint(latitude: origin.latitude + 0.0016, longitude: origin.longitude + 0.0008),
                CoordinatePoint(latitude: origin.latitude + 0.0025, longitude: origin.longitude + 0.0017),
                CoordinatePoint(latitude: origin.latitude + 0.0021, longitude: origin.longitude - 0.0006),
                CoordinatePoint(latitude: origin.latitude + 0.0009, longitude: origin.longitude - 0.0018),
                origin,
            ])
        }

        let latDelta = destination.latitude - origin.latitude
        let lonDelta = destination.longitude - origin.longitude
        let length = max(sqrt(latDelta * latDelta + lonDelta * lonDelta), 0.0001)
        let perpendicularLat = -lonDelta / length
        let perpendicularLon = latDelta / length
        let baseOffset = providerOffset(providerID) * (0.55 + Double(alternativeIndex) * 0.18)
        let fractions: [Double] = [0.12, 0.24, 0.39, 0.56, 0.73, 0.88]
        let patterns: [[Double]] = [
            [0.30, 0.58, 0.18, -0.08, 0.26, 0.05],
            [-0.22, -0.46, -0.12, -0.34, -0.08, 0.03],
            [0.18, 0.06, 0.42, 0.20, 0.34, 0.09],
        ]
        let pattern = patterns[min(alternativeIndex, patterns.count - 1)]

        var geometry: [CoordinatePoint] = [origin]
        for (index, fraction) in fractions.enumerated() {
            let lateral = baseOffset * pattern[index]
            let forwardBias = baseOffset * 0.12 * (Double(index) - 2.5)
            geometry.append(
                CoordinatePoint(
                    latitude: origin.latitude + latDelta * fraction + perpendicularLat * lateral + latDelta * forwardBias,
                    longitude: origin.longitude + lonDelta * fraction + perpendicularLon * lateral + lonDelta * forwardBias
                )
            )
        }
        geometry.append(destination)
        return deduplicated(geometry)
    }

    private func buildManeuvers(geometry: [CoordinatePoint]) -> [RouteManeuver] {
        let cumulative = cumulativeDistances(geometry)
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

    private func classifyTurn(deltaDegrees: Double) -> (type: RouteManeuverType, instruction: String)? {
        let magnitude = abs(deltaDegrees)
        guard magnitude >= 25 else { return nil }
        if magnitude >= 170 { return (.uturn, "Make a U-turn") }
        if magnitude >= 110 { return (deltaDegrees > 0 ? .sharpRight : .sharpLeft, deltaDegrees > 0 ? "Turn sharply right" : "Turn sharply left") }
        if magnitude >= 50 { return (deltaDegrees > 0 ? .right : .left, deltaDegrees > 0 ? "Turn right" : "Turn left") }
        return (deltaDegrees > 0 ? .slightRight : .slightLeft, deltaDegrees > 0 ? "Slight right" : "Slight left")
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
            return "Using sample OSM fallback routes. Live OSRM bike routing is unavailable."
        case .gpxImport:
            return "Using sample GPX import routes. File selection is not wired yet."
        case .fitImport:
            return "Using sample FIT import routes. File selection is not wired yet."
        case .tcxImport:
            return "Using sample TCX import routes. File selection is not wired yet."
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
        case .gpxImport:
            return 0.0014
        case .fitImport:
            return 0.0016
        case .tcxImport:
            return 0.0012
        case .hsl:
            return 0.0018
        }
    }

    private func providerAverageMetersPerSecond(_ provider: RouteProviderID) -> Double {
        switch provider {
        case .osm:
            return 5.2
        case .gpxImport:
            return 4.8
        case .fitImport:
            return 5.0
        case .tcxImport:
            return 4.7
        case .hsl:
            return 5.3
        }
    }

    private func importedFileDestination(from origin: CoordinatePoint) -> CoordinatePoint {
        let offset = providerOffset(providerID) * 2.0
        return CoordinatePoint(
            latitude: origin.latitude + offset,
            longitude: origin.longitude + offset * 0.7
        )
    }

    private func displayMessage(for error: Error) -> String {
        if let localized = error as? LocalizedError, let message = localized.errorDescription {
            return message
        }
        return error.localizedDescription
    }
}

enum SampleRoutingAdapterError: LocalizedError {
    case noAlternativesAvailable
    case invalidLiveRouteURL
    case networkFailure(String)
    case emptyImportFile

    var errorDescription: String? {
        switch self {
        case .noAlternativesAvailable:
            return "No sample route alternatives are available"
        case .invalidLiveRouteURL:
            return "The live OSM route URL is invalid"
        case .networkFailure(let message):
            return message
        case .emptyImportFile:
            return "The shared file was empty"
        }
    }
}

private extension SampleRoutingAdapter {
    struct OSRMRouteResponse: Decodable {
        var code: String
        var message: String?
        var routes: [OSRMRoute]
    }

    struct OSRMRoute: Decodable {
        var distance: Double
        var duration: Double
        var geometry: String
        var legs: [OSRMLeg]
    }

    struct OSRMLeg: Decodable {
        var summary: String?
        var steps: [OSRMStep]
    }

    struct OSRMStep: Decodable {
        var distance: Double
        var duration: Double
        var geometry: String?
        var name: String
        var maneuver: OSRMManeuver
    }

    struct OSRMManeuver: Decodable {
        var type: String
        var modifier: String?
        var location: [Double]
    }
}
