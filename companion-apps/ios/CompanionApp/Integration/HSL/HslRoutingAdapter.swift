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

    func replanRoute(
        using session: ActiveRouteSession,
        riderLocation: CoordinatePoint,
        rerouteContext: RerouteContext?
    ) async throws -> RoutePreviewModel {
        let rerouteOrigin = headingBiasedOrigin(
            riderLocation: riderLocation,
            rerouteContext: rerouteContext,
            providerLabel: "hsl"
        )
        let rerouteRequest = RoutePlanRequest(
            origin: rerouteOrigin,
            destination: session.destinationCoordinate ?? riderLocation,
            providerID: session.providerID
        )
        let revisionOverride = session.routeRevision.map { $0 + 1 }
        return try await planPreview(for: rerouteRequest, revisionOverride: revisionOverride)
    }

    private func headingBiasedOrigin(
        riderLocation: CoordinatePoint,
        rerouteContext: RerouteContext?,
        providerLabel: String
    ) -> CoordinatePoint {
        guard let heading = rerouteContext?.headingDegrees, heading.isFinite else {
            return riderLocation
        }
        guard let speed = rerouteContext?.speedMps, speed.isFinite, speed >= Self.minHeadingSpeedMps else {
            return riderLocation
        }
        let shifted = shiftPointByHeading(point: riderLocation, headingDegrees: heading, distanceMeters: Self.rerouteForwardShiftM)
        if !shifted.latitude.isFinite || !shifted.longitude.isFinite ||
            (shifted.latitude == riderLocation.latitude && shifted.longitude == riderLocation.longitude) {
            return riderLocation
        }
        return shifted
    }

    private func shiftPointByHeading(
        point: CoordinatePoint,
        headingDegrees: Double,
        distanceMeters: Double
    ) -> CoordinatePoint {
        let metersPerDegLat = 111_320.0
        let rad = headingDegrees * .pi / 180.0
        let northM = cos(rad) * distanceMeters
        let eastM = sin(rad) * distanceMeters
        let latitude = point.latitude + northM / metersPerDegLat
        let lonScale = metersPerDegLat * cos(point.latitude * .pi / 180.0)
        let longitude = lonScale == 0
            ? point.longitude
            : point.longitude + eastM / lonScale
        return CoordinatePoint(latitude: latitude, longitude: longitude)
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
        let liveResponse = try await fetchLiveDigitransitResponse(for: request, settings: settings)
        return normalizeResponse(
            liveResponse,
            request: request,
            revisionOverride: revisionOverride,
            planningNotice: "Live HSL Digitransit"
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
        let geometry = ShareImportUtilities.decodePolyline(leg.legGeometry?.points ?? "")
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
        let cyclingSpeedKph = settingsProvider().cyclingSpeedKph
        let alternatives = response.data.plan.itineraries.enumerated().map { index, itinerary in
            normalizeItinerary(
                itinerary,
                request: request,
                alternativeIndex: index,
                revision: revisionOverride ?? 1,
                cyclingSpeedKph: cyclingSpeedKph
            )
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
        revision: Int,
        cyclingSpeedKph: Double
    ) -> RouteAlternative {
        let routeID = buildRouteIdentifier(request: request, alternativeIndex: alternativeIndex)
        let geometry = deduplicatedGeometry(from: itinerary.legs)
        let maneuvers = buildManeuvers(from: itinerary, geometry: geometry)
        let totalDistance = itinerary.legs.reduce(0.0) { $0 + $1.distanceMeters }
        // Digitransit's bike speed is conservative for actual riders; recompute
        // the ETA from the user-set cycling speed so listed times match
        // real-world riding.
        let durationSeconds = Self.overrideDurationSeconds(
            totalDistanceMeters: totalDistance,
            cyclingSpeedKph: cyclingSpeedKph,
            fallbackSeconds: itinerary.durationSeconds
        )
        let summary = RouteSummary(
            totalDistanceMeters: totalDistance,
            estimatedDurationSeconds: durationSeconds,
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

    static func overrideDurationSeconds(
        totalDistanceMeters: Double,
        cyclingSpeedKph: Double,
        fallbackSeconds: Int
    ) -> Int {
        guard cyclingSpeedKph.isFinite, cyclingSpeedKph > 0 else { return fallbackSeconds }
        let mps = cyclingSpeedKph / 3.6
        return max(1, Int((totalDistanceMeters / mps).rounded()))
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
        let cumulative = cumulativeDistances(geometry)
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

    private func classifyTurn(deltaDegrees: Double) -> (token: String, instruction: String)? {
        let magnitude = abs(deltaDegrees)
        guard magnitude >= 25 else { return nil }
        if magnitude >= 170 { return (deltaDegrees > 0 ? "UTURN_RIGHT" : "UTURN_LEFT", "Make a U-turn") }
        if magnitude >= 110 { return (deltaDegrees > 0 ? "HARD_RIGHT" : "HARD_LEFT", deltaDegrees > 0 ? "Turn sharply right" : "Turn sharply left") }
        if magnitude >= 50 { return (deltaDegrees > 0 ? "RIGHT" : "LEFT", deltaDegrees > 0 ? "Turn right" : "Turn left") }
        return (deltaDegrees > 0 ? "SLIGHTLY_RIGHT" : "SLIGHTLY_LEFT", deltaDegrees > 0 ? "Slight right" : "Slight left")
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

    private static let minHeadingSpeedMps: Double = 2.0
    private static let rerouteForwardShiftM: Double = 15.0

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

