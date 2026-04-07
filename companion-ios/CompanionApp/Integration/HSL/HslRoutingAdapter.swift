import Foundation

struct HslRoutingAdapter: RoutingProvider {
    let providerID: RouteProviderID = .hsl
    let isAvailableInV1: Bool = true

    private let settingsProvider: () -> CompanionSettings

    init(settingsProvider: @escaping () -> CompanionSettings = { .defaults }) {
        self.settingsProvider = settingsProvider
    }

    func planRoute(_ request: RoutePlanRequest) async throws -> RoutePreviewModel {
        try await planPreview(for: request, revisionOverride: nil)
    }

    func replanRoute(using session: ActiveRouteSession, riderLocation: CoordinatePoint) async throws -> RoutePreviewModel {
        let rerouteRequest = RoutePlanRequest(
            origin: riderLocation,
            destination: session.destinationCoordinate ?? riderLocation,
            providerID: session.providerID
        )
        let revisionOverride = session.routeRevision.map { $0 + 1 }
        return try await planPreview(for: rerouteRequest, revisionOverride: revisionOverride)
    }

    func normalizePreview(_ preview: RoutePreviewModel, request: RoutePlanRequest) throws -> NormalizedRoutePackage {
        guard let selected = preview.selectedAlternative else {
            throw HslRoutingAdapterError.noAlternativesAvailable
        }
        return selected.normalizedPackage
    }

    private func planPreview(
        for request: RoutePlanRequest,
        revisionOverride: Int?
    ) async throws -> RoutePreviewModel {
        let settings = settingsProvider()
        if settings.preferLiveHslRouting {
            let trimmedKey = settings.hslSubscriptionKey.trimmingCharacters(in: .whitespacesAndNewlines)
            if trimmedKey.isEmpty {
                return normalizeResponse(
                    sampleDigitransitResponse(for: request, liveDescriptor: "Fallback sample: missing HSL subscription key"),
                    request: request,
                    revisionOverride: revisionOverride,
                    planningNotice: "No HSL subscription key configured. Showing sample route instead."
                )
            }
            do {
                let liveResponse = try await fetchLiveDigitransitResponse(for: request, settings: settings)
                return normalizeResponse(
                    liveResponse,
                    request: request,
                    revisionOverride: revisionOverride,
                    planningNotice: "Live HSL Digitransit"
                )
            } catch {
                return normalizeResponse(
                    sampleDigitransitResponse(for: request, liveDescriptor: "Fallback sample after live HSL failure"),
                    request: request,
                    revisionOverride: revisionOverride,
                    planningNotice: "Live HSL failed: \(displayMessage(for: error)). Showing sample route instead."
                )
            }
        }

        return normalizeResponse(
            sampleDigitransitResponse(for: request, liveDescriptor: "Sample HSL route"),
            request: request,
            revisionOverride: revisionOverride,
            planningNotice: "Using sample HSL routes. Enable live HSL in Settings."
        )
    }

    func makeGraphQLRequestBody(for request: RoutePlanRequest) -> DigitransitGraphQLRequestBody {
        DigitransitGraphQLRequestBody(
            query: Self.routePlanQuery,
            variables: .init(
                from: .init(lat: request.origin.latitude, lon: request.origin.longitude),
                to: .init(lat: request.destination.latitude, lon: request.destination.longitude),
                numItineraries: 3,
                transportModes: [.init(mode: "BICYCLE")],
                optimize: "SAFE"
            )
        )
    }

    private func fetchLiveDigitransitResponse(
        for request: RoutePlanRequest,
        settings: CompanionSettings
    ) async throws -> DigitransitResponse {
        guard let url = URL(string: settings.hslEndpointURL) else {
            throw HslRoutingAdapterError.invalidEndpoint(settings.hslEndpointURL)
        }

        var urlRequest = URLRequest(url: url)
        urlRequest.httpMethod = "POST"
        urlRequest.setValue("application/json", forHTTPHeaderField: "content-type")
        urlRequest.setValue(settings.hslSubscriptionKey, forHTTPHeaderField: "digitransit-subscription-key")
        urlRequest.httpBody = try JSONEncoder().encode(makeGraphQLRequestBody(for: request))

        let (data, response) = try await URLSession.shared.data(for: urlRequest)
        guard let http = response as? HTTPURLResponse else {
            throw HslRoutingAdapterError.networkFailure("Missing HTTP response")
        }
        guard (200 ..< 300).contains(http.statusCode) else {
            let bodyMessage = String(data: data, encoding: .utf8) ?? "No response body"
            throw HslRoutingAdapterError.networkFailure("HTTP \(http.statusCode): \(bodyMessage)")
        }

        let decoded = try JSONDecoder().decode(DigitransitApiResponse.self, from: data)
        if let errors = decoded.errors, !errors.isEmpty {
            throw HslRoutingAdapterError.networkFailure(errors.map(\.message).joined(separator: " | "))
        }
        guard let plan = decoded.data?.plan, !plan.itineraries.isEmpty else {
            throw HslRoutingAdapterError.noAlternativesAvailable
        }

        let itineraries = plan.itineraries.enumerated().map { index, itinerary in
            let legs = itinerary.legs.compactMap { liveLeg in
                mapLiveLeg(liveLeg)
            }
            return DigitransitItinerary(
                durationSeconds: Int(itinerary.duration.rounded()),
                systemNotice: index == 0 ? "HSL Digitransit live / fastest" : "HSL Digitransit live / alternative",
                legs: legs,
                steps: [],
                startLabel: itinerary.legs.first?.from?.name ?? "Current location",
                destinationLabel: itinerary.legs.last?.to?.name ?? "Selected destination"
            )
        }

        return DigitransitResponse(
            data: DataContainer(
                plan: DigitransitPlan(itineraries: itineraries)
            )
        )
    }

    private func mapLiveLeg(_ leg: LiveLeg) -> DigitransitLeg? {
        let geometry = decodePolyline(leg.legGeometry?.points ?? "")
        let fallback = fallbackGeometry(from: leg)
        let points = geometry.count >= 2 ? geometry : fallback
        guard points.count >= 2 else { return nil }
        return DigitransitLeg(
            mode: leg.mode ?? "BICYCLE",
            distanceMeters: leg.distance,
            geometry: points
        )
    }

    private func fallbackGeometry(from leg: LiveLeg) -> [CoordinatePoint] {
        var points: [CoordinatePoint] = []
        if let from = leg.from {
            points.append(CoordinatePoint(latitude: from.lat, longitude: from.lon))
        }
        if let to = leg.to {
            let point = CoordinatePoint(latitude: to.lat, longitude: to.lon)
            if points.last != point {
                points.append(point)
            }
        }
        return points
    }

    private func normalizeResponse(
        _ response: DigitransitResponse,
        request: RoutePlanRequest,
        revisionOverride: Int?,
        planningNotice: String?
    ) -> RoutePreviewModel {
        let alternatives = response.data.plan.itineraries.enumerated().map { index, itinerary in
            normalizeItinerary(itinerary, request: request, alternativeIndex: index, revision: revisionOverride ?? 1)
        }
        return RoutePreviewModel(
            alternatives: alternatives,
            selectedAlternativeID: alternatives.first?.id,
            routeIdentifier: alternatives.first?.normalizedPackage.routeIdentifier,
            routeRevision: alternatives.first?.normalizedPackage.revision,
            planningNotice: planningNotice
        )
    }

    private func normalizeItinerary(
        _ itinerary: DigitransitItinerary,
        request: RoutePlanRequest,
        alternativeIndex: Int,
        revision: Int
    ) -> RouteAlternative {
        let routeID = buildRouteIdentifier(request: request, alternativeIndex: alternativeIndex)
        let geometry = deduplicatedGeometry(from: itinerary.legs)
        let maneuvers = buildManeuvers(from: itinerary, geometry: geometry)
        let totalDistance = itinerary.legs.reduce(0.0) { $0 + $1.distanceMeters }
        let summary = RouteSummary(
            totalDistanceMeters: totalDistance,
            estimatedDurationSeconds: itinerary.durationSeconds,
            startLabel: itinerary.startLabel,
            destinationLabel: itinerary.destinationLabel
        )
        let package = NormalizedRoutePackage(
            version: .current,
            routeIdentifier: routeID,
            revision: revision,
            geometry: geometry,
            maneuvers: maneuvers,
            summary: summary,
            provenance: RouteProvenance(
                providerID: .hsl,
                sourceReference: itinerary.systemNotice,
                generatedAtUnixMs: UInt64(Date().timeIntervalSince1970 * 1000)
            )
        )
        return RouteAlternative(
            id: UUID(),
            title: alternativeIndex == 0 ? "Fastest bike route" : "Alternative bike route",
            subtitle: itinerary.systemNotice,
            distanceMeters: Int(summary.totalDistanceMeters.rounded()),
            durationSeconds: summary.estimatedDurationSeconds,
            normalizedPackage: package
        )
    }

    private func buildManeuvers(
        from itinerary: DigitransitItinerary,
        geometry: [CoordinatePoint]
    ) -> [RouteManeuver] {
        let routeDistance = itinerary.legs.reduce(0.0) { $0 + $1.distanceMeters }
        let steps = itinerary.steps.isEmpty ? deriveSteps(from: geometry) : itinerary.steps
        var maneuvers: [RouteManeuver] = [
            RouteManeuver(
                id: "depart",
                maneuverType: .depart,
                location: geometry.first ?? CoordinatePoint(latitude: 0.0, longitude: 0.0),
                distanceFromStartMeters: 0.0,
                distanceToNextMeters: steps.first?.distanceFromStartMeters,
                instructionText: "Start riding"
            )
        ]

        for (index, step) in steps.enumerated() {
            maneuvers.append(
                RouteManeuver(
                    id: "step-\(index)",
                    maneuverType: maneuverType(for: step.relativeDirection),
                    location: step.location,
                    distanceFromStartMeters: step.distanceFromStartMeters,
                    distanceToNextMeters: step.distanceToNextMeters,
                    instructionText: step.instruction
                )
            )
        }

        maneuvers.append(
            RouteManeuver(
                id: "arrive",
                maneuverType: .arrive,
                location: geometry.last ?? CoordinatePoint(latitude: 0.0, longitude: 0.0),
                distanceFromStartMeters: routeDistance,
                distanceToNextMeters: nil,
                instructionText: "Arrive at destination"
            )
        )

        return maneuvers
    }

    private func deriveSteps(from geometry: [CoordinatePoint]) -> [DigitransitStep] {
        guard geometry.count >= 3 else { return [] }
        let cumulative = cumulativeDistances(for: geometry)
        var steps: [DigitransitStep] = []
        for index in 1 ..< geometry.count - 1 {
            let turnDelta = turnDeltaDegrees(previous: geometry[index - 1], current: geometry[index], next: geometry[index + 1])
            guard let classification = classifyTurn(deltaDegrees: turnDelta) else { continue }
            let distanceToNext = index + 1 < cumulative.count ? cumulative[index + 1] - cumulative[index] : nil
            steps.append(
                DigitransitStep(
                    relativeDirection: classification.token,
                    location: geometry[index],
                    distanceFromStartMeters: cumulative[index],
                    distanceToNextMeters: distanceToNext,
                    instruction: classification.instruction
                )
            )
        }
        return steps
    }

    private func cumulativeDistances(for geometry: [CoordinatePoint]) -> [Double] {
        var cumulative: [Double] = [0.0]
        for (start, end) in zip(geometry, geometry.dropFirst()) {
            cumulative.append(cumulative.last! + approximateDistanceMeters(from: start, to: end))
        }
        return cumulative
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
        let latScale = 111_320.0
        let lonScale = cos(((start.latitude + end.latitude) / 2) * .pi / 180.0) * 111_320.0
        let latMeters = (end.latitude - start.latitude) * latScale
        let lonMeters = (end.longitude - start.longitude) * lonScale
        return atan2(lonMeters, latMeters) * 180.0 / .pi
    }

    private func classifyTurn(deltaDegrees: Double) -> (token: String, instruction: String)? {
        let magnitude = abs(deltaDegrees)
        guard magnitude >= 25 else { return nil }
        if magnitude >= 170 { return (deltaDegrees > 0 ? "UTURN_RIGHT" : "UTURN_LEFT", "Make a U-turn") }
        if magnitude >= 110 { return (deltaDegrees > 0 ? "HARD_RIGHT" : "HARD_LEFT", deltaDegrees > 0 ? "Turn sharply right" : "Turn sharply left") }
        if magnitude >= 50 { return (deltaDegrees > 0 ? "RIGHT" : "LEFT", deltaDegrees > 0 ? "Turn right" : "Turn left") }
        return (deltaDegrees > 0 ? "SLIGHTLY_RIGHT" : "SLIGHTLY_LEFT", deltaDegrees > 0 ? "Bear right" : "Bear left")
    }

    private func deduplicatedGeometry(from legs: [DigitransitLeg]) -> [CoordinatePoint] {
        var points: [CoordinatePoint] = []
        for point in legs.flatMap(\.geometry) {
            if points.last != point {
                points.append(point)
            }
        }
        return points
    }

    private func buildRouteIdentifier(request: RoutePlanRequest, alternativeIndex: Int) -> String {
        let origin = String(format: "%.5f,%.5f", request.origin.latitude, request.origin.longitude)
        let destination = String(format: "%.5f,%.5f", request.destination.latitude, request.destination.longitude)
        return "hsl:\(origin)->\(destination):alt-\(alternativeIndex)"
    }

    private func maneuverType(for relativeDirection: String) -> RouteManeuverType {
        switch relativeDirection.uppercased() {
        case "CONTINUE":
            return .straight
        case "SLIGHTLY_LEFT":
            return .slightLeft
        case "LEFT":
            return .left
        case "HARD_LEFT":
            return .sharpLeft
        case "SLIGHTLY_RIGHT":
            return .slightRight
        case "RIGHT":
            return .right
        case "HARD_RIGHT":
            return .sharpRight
        case "UTURN_LEFT", "UTURN_RIGHT", "UTURN":
            return .uturn
        case "CIRCLE_COUNTERCLOCKWISE", "CIRCLE_CLOCKWISE":
            return .roundabout
        case "ELEVATOR", "TRANSFER":
            return .ramp
        default:
            return .straight
        }
    }

    private func decodePolyline(_ encoded: String) -> [CoordinatePoint] {
        guard !encoded.isEmpty else { return [] }
        var points: [CoordinatePoint] = []
        var index = encoded.startIndex
        var latitude = 0
        var longitude = 0

        func nextValue() -> Int? {
            var result = 0
            var shift = 0
            while index < encoded.endIndex {
                let value = Int(encoded[index].unicodeScalars.first!.value) - 63
                index = encoded.index(after: index)
                result |= (value & 0x1f) << shift
                shift += 5
                if value < 0x20 {
                    return (result & 1) == 0 ? (result >> 1) : ~(result >> 1)
                }
            }
            return nil
        }

        while let latDelta = nextValue(), let lonDelta = nextValue() {
            latitude += latDelta
            longitude += lonDelta
            points.append(
                CoordinatePoint(
                    latitude: Double(latitude) / 100_000.0,
                    longitude: Double(longitude) / 100_000.0
                )
            )
        }

        return points
    }

    private func displayMessage(for error: Error) -> String {
        if let localized = error as? LocalizedError, let message = localized.errorDescription {
            return message
        }
        return error.localizedDescription
    }

    func sampleDigitransitResponse(for request: RoutePlanRequest, liveDescriptor: String) -> DigitransitResponse {
        let origin = request.origin
        let destination = request.destination
        let itineraries = [
            ("fastest", sampleGeometry(from: origin, to: destination, alternativeIndex: 0, offsetScale: 0.0013)),
            ("quieter", sampleGeometry(from: origin, to: destination, alternativeIndex: 1, offsetScale: 0.0016)),
            ("simpler", sampleGeometry(from: origin, to: destination, alternativeIndex: 2, offsetScale: 0.0010)),
        ].map { descriptor, geometry in
            makeItinerary(
                systemNotice: "\(liveDescriptor) / \(descriptor)",
                geometry: geometry,
                startLabel: "Current location",
                destinationLabel: "Selected destination"
            )
        }

        return DigitransitResponse(
            data: DataContainer(
                plan: DigitransitPlan(itineraries: itineraries)
            )
        )
    }

    private func makeItinerary(
        systemNotice: String,
        geometry: [CoordinatePoint],
        startLabel: String,
        destinationLabel: String
    ) -> DigitransitItinerary {
        let segmentDistances = zip(geometry, geometry.dropFirst()).map { start, end in
            approximateDistanceMeters(from: start, to: end)
        }
        let totalDistance = segmentDistances.reduce(0, +)

        return DigitransitItinerary(
            durationSeconds: Int((totalDistance / 4.2).rounded()),
            systemNotice: systemNotice,
            legs: [DigitransitLeg(mode: "BICYCLE", distanceMeters: totalDistance, geometry: geometry)],
            steps: [],
            startLabel: startLabel,
            destinationLabel: destinationLabel
        )
    }

    private func sampleGeometry(from origin: CoordinatePoint, to destination: CoordinatePoint, alternativeIndex: Int, offsetScale: Double) -> [CoordinatePoint] {
        if origin == destination {
            return [
                origin,
                CoordinatePoint(latitude: origin.latitude + 0.0015, longitude: origin.longitude + 0.0009),
                CoordinatePoint(latitude: origin.latitude + 0.0024, longitude: origin.longitude + 0.0016),
                CoordinatePoint(latitude: origin.latitude + 0.0019, longitude: origin.longitude - 0.0004),
                CoordinatePoint(latitude: origin.latitude + 0.0008, longitude: origin.longitude - 0.0016),
                origin,
            ]
        }

        let latDelta = destination.latitude - origin.latitude
        let lonDelta = destination.longitude - origin.longitude
        let length = max((latDelta * latDelta + lonDelta * lonDelta).squareRoot(), 0.0001)
        let perpendicularLat = -lonDelta / length
        let perpendicularLon = latDelta / length
        let fractions: [Double] = [0.10, 0.22, 0.38, 0.54, 0.72, 0.88]
        let patterns: [[Double]] = [
            [0.26, 0.52, 0.22, 0.00, 0.16, 0.04],
            [-0.20, -0.42, -0.18, -0.28, -0.10, 0.02],
            [0.12, 0.06, 0.28, 0.14, 0.24, 0.08],
        ]
        let pattern = patterns[min(alternativeIndex, patterns.count - 1)]

        var geometry: [CoordinatePoint] = [origin]
        for (index, fraction) in fractions.enumerated() {
            let lateral = offsetScale * pattern[index]
            let forwardBias = offsetScale * 0.10 * (Double(index) - 2.5)
            geometry.append(
                CoordinatePoint(
                    latitude: origin.latitude + latDelta * fraction + perpendicularLat * lateral + latDelta * forwardBias,
                    longitude: origin.longitude + lonDelta * fraction + perpendicularLon * lateral + lonDelta * forwardBias
                )
            )
        }
        geometry.append(destination)
        return geometry
    }

    private func approximateDistanceMeters(from start: CoordinatePoint, to end: CoordinatePoint) -> Double {
        let latScale = 111_320.0
        let lonScale = cos(((start.latitude + end.latitude) / 2) * .pi / 180.0) * 111_320.0
        let latMeters = (end.latitude - start.latitude) * latScale
        let lonMeters = (end.longitude - start.longitude) * lonScale
        return (latMeters * latMeters + lonMeters * lonMeters).squareRoot()
    }
}

enum HslRoutingAdapterError: LocalizedError {
    case noAlternativesAvailable
    case invalidEndpoint(String)
    case networkFailure(String)

    var errorDescription: String? {
        switch self {
        case .noAlternativesAvailable:
            return "No HSL route alternatives were returned."
        case .invalidEndpoint(let endpoint):
            return "Invalid HSL endpoint: \(endpoint)"
        case .networkFailure(let message):
            return message
        }
    }
}

extension HslRoutingAdapter {
    struct DigitransitGraphQLRequestBody: Encodable {
        var query: String
        var variables: Variables

        struct Variables: Encodable {
            var from: CoordinateVariable
            var to: CoordinateVariable
            var numItineraries: Int
            var transportModes: [TransportMode]
            var optimize: String
        }

        struct CoordinateVariable: Encodable {
            var lat: Double
            var lon: Double
        }

        struct TransportMode: Encodable {
            var mode: String
        }
    }

    struct DigitransitApiResponse: Decodable {
        var data: LiveDataContainer?
        var errors: [LiveError]?
    }

    struct LiveError: Decodable {
        var message: String
    }

    struct LiveDataContainer: Decodable {
        var plan: LivePlan?
    }

    struct LivePlan: Decodable {
        var itineraries: [LiveItinerary]
    }

    struct LiveItinerary: Decodable {
        var duration: Double
        var legs: [LiveLeg]
    }

    struct LiveLeg: Decodable {
        var mode: String?
        var distance: Double
        var from: LivePlace?
        var to: LivePlace?
        var legGeometry: LiveLegGeometry?
    }

    struct LivePlace: Decodable {
        var lat: Double
        var lon: Double
        var name: String?
    }

    struct LiveLegGeometry: Decodable {
        var points: String
    }

    struct DigitransitResponse {
        var data: DataContainer
    }

    struct DataContainer {
        var plan: DigitransitPlan
    }

    struct DigitransitPlan {
        var itineraries: [DigitransitItinerary]
    }

    struct DigitransitItinerary {
        var durationSeconds: Int
        var systemNotice: String
        var legs: [DigitransitLeg]
        var steps: [DigitransitStep]
        var startLabel: String
        var destinationLabel: String
    }

    struct DigitransitLeg {
        var mode: String
        var distanceMeters: Double
        var geometry: [CoordinatePoint]
    }

    struct DigitransitStep {
        var relativeDirection: String
        var location: CoordinatePoint
        var distanceFromStartMeters: Double
        var distanceToNextMeters: Double?
        var instruction: String
    }

    static let routePlanQuery = """
    query RoutePlan($from: InputCoordinates!, $to: InputCoordinates!, $numItineraries: Int!, $transportModes: [TransportMode!]!, $optimize: OptimizeType!) {
      plan(
        from: $from,
        to: $to,
        numItineraries: $numItineraries,
        transportModes: $transportModes,
        optimize: $optimize
      ) {
        itineraries {
          duration
          legs {
            mode
            distance
            from {
              lat
              lon
              name
            }
            to {
              lat
              lon
              name
            }
            legGeometry {
              points
            }
          }
        }
      }
    }
    """
}
