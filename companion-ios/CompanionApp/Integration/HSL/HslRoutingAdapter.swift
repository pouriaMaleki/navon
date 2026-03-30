import Foundation

struct HslRoutingAdapter: RoutingProvider {
    let providerID: RouteProviderID = .hsl
    let isAvailableInV1: Bool = true

    func planRoute(_ request: RoutePlanRequest) async throws -> RoutePreviewModel {
        let response = sampleDigitransitResponse(for: request)
        return normalizeResponse(response, request: request)
    }

    func replanRoute(using session: ActiveRouteSession, riderLocation: CoordinatePoint) async throws -> RoutePreviewModel {
        let rerouteRequest = RoutePlanRequest(
            origin: riderLocation,
            destination: session.destinationCoordinate ?? riderLocation,
            providerID: session.providerID
        )
        let response = sampleDigitransitResponse(for: rerouteRequest)
        var preview = normalizeResponse(response, request: rerouteRequest)
        if let routeRevision = session.routeRevision {
            preview.routeRevision = routeRevision + 1
        }
        return preview
    }

    func normalizePreview(_ preview: RoutePreviewModel, request: RoutePlanRequest) throws -> NormalizedRoutePackage {
        guard let selected = preview.selectedAlternative else {
            throw HslRoutingAdapterError.noAlternativesAvailable
        }
        return selected.normalizedPackage
    }

    func makeGraphQLRequestBody(for request: RoutePlanRequest) -> DigitransitGraphQLRequestBody {
        DigitransitGraphQLRequestBody(
            query: Self.routePlanQuery,
            variables: .init(
                from: .init(lat: request.origin.latitude, lon: request.origin.longitude),
                to: .init(lat: request.destination.latitude, lon: request.destination.longitude),
                numItineraries: 2,
                transportModes: [.init(mode: "BICYCLE")],
                optimize: "SAFE"
            )
        )
    }

    private func normalizeResponse(
        _ response: DigitransitResponse,
        request: RoutePlanRequest
    ) -> RoutePreviewModel {
        let alternatives = response.data.plan.itineraries.enumerated().map { index, itinerary in
            normalizeItinerary(itinerary, request: request, alternativeIndex: index)
        }
        return RoutePreviewModel(
            alternatives: alternatives,
            selectedAlternativeID: alternatives.first?.id,
            routeIdentifier: alternatives.first?.normalizedPackage.routeIdentifier,
            routeRevision: alternatives.first?.normalizedPackage.revision
        )
    }

    private func normalizeItinerary(
        _ itinerary: DigitransitItinerary,
        request: RoutePlanRequest,
        alternativeIndex: Int
    ) -> RouteAlternative {
        let routeID = buildRouteIdentifier(request: request, alternativeIndex: alternativeIndex)
        let geometry = deduplicatedGeometry(from: itinerary.legs)
        let maneuvers = buildManeuvers(from: itinerary, geometry: geometry)
        let summary = RouteSummary(
            totalDistanceMeters: itinerary.legs.reduce(0.0) { $0 + $1.distanceMeters },
            estimatedDurationSeconds: itinerary.durationSeconds,
            startLabel: "Current location",
            destinationLabel: "Selected destination"
        )
        let package = NormalizedRoutePackage(
            version: .current,
            routeIdentifier: routeID,
            revision: 1,
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
            title: alternativeIndex == 0 ? "Fastest bike route" : "Quieter streets",
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
        var maneuvers: [RouteManeuver] = [
            RouteManeuver(
                id: "depart",
                maneuverType: .depart,
                location: geometry.first ?? CoordinatePoint(latitude: 0.0, longitude: 0.0),
                distanceFromStartMeters: 0.0,
                distanceToNextMeters: itinerary.steps.first?.distanceFromStartMeters,
                instructionText: "Start riding"
            )
        ]

        for (index, step) in itinerary.steps.enumerated() {
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
}

enum HslRoutingAdapterError: Error {
    case noAlternativesAvailable
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
          systemNotices
          legs {
            mode
            distance
            legGeometry {
              points
            }
          }
        }
      }
    }
    """

    func sampleDigitransitResponse(for request: RoutePlanRequest) -> DigitransitResponse {
        let origin = request.origin
        let destination = request.destination
        let midpointA = CoordinatePoint(
            latitude: (origin.latitude + destination.latitude) / 2 + 0.0020,
            longitude: (origin.longitude + destination.longitude) / 2 - 0.0012
        )
        let midpointB = CoordinatePoint(
            latitude: (origin.latitude + destination.latitude) / 2 - 0.0010,
            longitude: (origin.longitude + destination.longitude) / 2 + 0.0015
        )
        let midpointC = CoordinatePoint(
            latitude: origin.latitude + (destination.latitude - origin.latitude) * 0.75 + 0.0008,
            longitude: origin.longitude + (destination.longitude - origin.longitude) * 0.72 - 0.0010
        )

        let fastestGeometry = [origin, midpointA, midpointC, destination]
        let quieterGeometry = [origin, midpointB, midpointC, destination]

        return DigitransitResponse(
            data: DataContainer(
                plan: DigitransitPlan(
                    itineraries: [
                        makeItinerary(
                            systemNotice: "HSL Digitransit bike / fastest",
                            geometry: fastestGeometry,
                            turnInstructions: ["RIGHT", "LEFT"]
                        ),
                        makeItinerary(
                            systemNotice: "HSL Digitransit bike / quieter",
                            geometry: quieterGeometry,
                            turnInstructions: ["LEFT", "RIGHT"]
                        )
                    ]
                )
            )
        )
    }

    private func makeItinerary(
        systemNotice: String,
        geometry: [CoordinatePoint],
        turnInstructions: [String]
    ) -> DigitransitItinerary {
        let segmentDistances = zip(geometry, geometry.dropFirst()).map { start, end in
            approximateDistanceMeters(from: start, to: end)
        }
        let totalDistance = segmentDistances.reduce(0, +)
        var distanceFromStart = segmentDistances.first ?? 0.0
        let turnLocations = Array(geometry.dropFirst().dropLast())
        let steps = zip(turnLocations.indices, turnLocations).map { index, point in
            let distanceToNext = index < segmentDistances.count - 1 ? segmentDistances[index + 1] : nil
            defer { distanceFromStart += segmentDistances.dropFirst(index + 1).first ?? 0.0 }
            return DigitransitStep(
                relativeDirection: turnInstructions[index],
                location: point,
                distanceFromStartMeters: distanceFromStart,
                distanceToNextMeters: distanceToNext,
                instruction: turnInstructions[index] == "LEFT" ? "Turn left" : "Turn right"
            )
        }

        return DigitransitItinerary(
            durationSeconds: Int((totalDistance / 4.2).rounded()),
            systemNotice: systemNotice,
            legs: [DigitransitLeg(mode: "BICYCLE", distanceMeters: totalDistance, geometry: geometry)],
            steps: steps
        )
    }

    private func approximateDistanceMeters(from start: CoordinatePoint, to end: CoordinatePoint) -> Double {
        let latScale = 111_320.0
        let lonScale = cos(((start.latitude + end.latitude) / 2) * .pi / 180.0) * 111_320.0
        let latMeters = (end.latitude - start.latitude) * latScale
        let lonMeters = (end.longitude - start.longitude) * lonScale
        return (latMeters * latMeters + lonMeters * lonMeters).squareRoot()
    }
}
