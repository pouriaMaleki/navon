import Foundation

/// Fans out to BRouter (fastbike + trekking) and OSRM in parallel, exposing whichever succeed as alternatives.
struct OsmCyclingRoutingAdapter: RoutingProvider {
    let providerID: RouteProviderID = .osm
    let isAvailableInV1: Bool = true
    private static let minHeadingSpeedMps: Double = 2.0
    private static let rerouteForwardShiftM: Double = 15.0

    /// Last-resort fallback when all live sources fail.
    private let sample = SampleRoutingAdapter(providerID: .osm)

    func planRoute(_ request: RoutePlanRequest) async throws -> RoutePreviewModel {
        return await fanOutOrFallback(request: request, revision: 1)
    }

    func replanRoute(
        using session: ActiveRouteSession,
        riderLocation: CoordinatePoint,
        rerouteContext: RerouteContext?
    ) async throws -> RoutePreviewModel {
        let rerouteOrigin = headingBiasedOrigin(
            riderLocation: riderLocation,
            rerouteContext: rerouteContext,
            providerLabel: "osm"
        )
        let request = RoutePlanRequest(
            origin: rerouteOrigin,
            destination: session.destinationCoordinate ?? riderLocation,
            providerID: providerID
        )
        return await fanOutOrFallback(request: request, revision: (session.routeRevision ?? 0) + 1)
    }

    func normalizePreview(_ preview: RoutePreviewModel, request: RoutePlanRequest) throws -> NormalizedRoutePackage {
        guard let selected = preview.selectedAlternative else {
            throw NSError(domain: "OsmCycling", code: 1, userInfo: [NSLocalizedDescriptionKey: "no alternatives"])
        }
        return selected.normalizedPackage
    }

    private func fanOutOrFallback(request: RoutePlanRequest, revision: Int) async -> RoutePreviewModel {
        async let fastbike = mapBrouter(request: request, revision: revision, profile: .fastbike, title: "Bike-paths first")
        async let trekking = mapBrouter(request: request, revision: revision, profile: .trekking, title: "Balanced cycling")
        async let osrm = fetchOsrmAlternative(request: request, revision: revision)
        let results: [RouteAlternative?] = [await fastbike, await trekking, await osrm]
        let successes = results.compactMap { $0 }
        let deduped = dedupeAlternatives(successes)
        if deduped.isEmpty {
            let fallback = (try? await sample.planRoute(request)) ?? RoutePreviewModel(
                alternatives: [], selectedAlternativeID: nil,
                routeIdentifier: nil, routeRevision: nil,
                planningNotice: T.string("cycling.sampleFallback")
            )
            return RoutePreviewModel(
                alternatives: fallback.alternatives,
                selectedAlternativeID: fallback.selectedAlternativeID,
                routeIdentifier: fallback.routeIdentifier,
                routeRevision: fallback.routeRevision,
                planningNotice: T.string("cycling.sampleFallback")
            )
        }
        let failedCount = results.count - deduped.count
        let notice = failedCount == 0
            ? T.string("cycling.alternativesLive")
            : T.string("cycling.alternativesPartial", ["count": .number(Double(failedCount))])
        return RoutePreviewModel(
            alternatives: deduped,
            selectedAlternativeID: deduped.first?.id,
            routeIdentifier: deduped.first?.normalizedPackage.routeIdentifier,
            routeRevision: deduped.first?.normalizedPackage.revision,
            planningNotice: notice
        )
    }

    private func mapBrouter(
        request: RoutePlanRequest,
        revision: Int,
        profile: BrouterProfile,
        title: String
    ) async -> RouteAlternative? {
        do {
            let feature = try await BrouterClient.fetch(profile: profile, origin: request.origin, destination: request.destination)
            return mapBrouterToAlternative(feature: feature, request: request, revision: revision, profile: profile, title: title)
        } catch {
            return nil
        }
    }

    private func fetchOsrmAlternative(request: RoutePlanRequest, revision: Int) async -> RouteAlternative? {
        let coordinates = String(
            format: "%.6f,%.6f;%.6f,%.6f",
            request.origin.longitude, request.origin.latitude,
            request.destination.longitude, request.destination.latitude
        )
        guard let url = URL(string: "https://router.project-osrm.org/route/v1/bike/\(coordinates)?alternatives=false&overview=full&steps=true&geometries=geojson") else {
            return nil
        }
        do {
            let (data, response) = try await URLSession.shared.data(from: url)
            guard let http = response as? HTTPURLResponse, (200..<300).contains(http.statusCode) else {
                return nil
            }
            guard let root = try JSONSerialization.jsonObject(with: data) as? [String: Any],
                  (root["code"] as? String) == "Ok",
                  let routes = root["routes"] as? [[String: Any]],
                  let first = routes.first else {
                return nil
            }
            return mapOsrmRoute(routeJson: first, request: request, revision: revision)
        } catch {
            return nil
        }
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
}

private func mapOsrmRoute(routeJson: [String: Any], request: RoutePlanRequest, revision: Int) -> RouteAlternative? {
    guard let geom = routeJson["geometry"] as? [String: Any],
          let coordsRaw = geom["coordinates"] as? [[Double]] else { return nil }
    let geometry: [CoordinatePoint] = coordsRaw.compactMap { c in
        guard c.count >= 2 else { return nil }
        return CoordinatePoint(latitude: c[1], longitude: c[0])
    }
    guard geometry.count >= 2 else { return nil }
    let distance = (routeJson["distance"] as? Double) ?? 0.0
    let duration = max(Int((routeJson["duration"] as? Double) ?? 0), 60)
    let maneuvers = buildOsrmManeuvers(routeJson: routeJson, geometry: geometry)
    let routePackage = NormalizedRoutePackage(
        version: RoutePackageVersion.current,
        routeIdentifier: osrmRouteId(request: request),
        revision: revision,
        geometry: geometry,
        maneuvers: maneuvers,
        summary: RouteSummary(
            totalDistanceMeters: distance,
            estimatedDurationSeconds: duration,
            startLabel: "Current location",
            destinationLabel: "Selected destination"
        ),
        provenance: RouteProvenance(
            providerID: .osm,
            sourceReference: "OSRM bike",
            generatedAtUnixMs: UInt64(Date().timeIntervalSince1970 * 1000)
        )
    )
    let km = String(format: "%.1f", distance / 1000.0)
    let min = max(duration / 60, 1)
    return RouteAlternative(
        id: UUID(),
        title: "Fastest",
        subtitle: "\(km) km • \(min) min",
        distanceMeters: Int(distance),
        durationSeconds: duration,
        normalizedPackage: routePackage
    )
}

private func buildOsrmManeuvers(routeJson: [String: Any], geometry: [CoordinatePoint]) -> [RouteManeuver] {
    var maneuvers: [RouteManeuver] = []
    let legs = routeJson["legs"] as? [[String: Any]] ?? []
    let firstNext = firstOsrmStepDistance(legs: legs)
    maneuvers.append(RouteManeuver(
        id: "depart", maneuverType: .depart,
        location: geometry.first ?? CoordinatePoint(latitude: 0, longitude: 0),
        distanceFromStartMeters: 0, distanceToNextMeters: firstNext,
        instructionText: "Start riding"
    ))
    var distanceFromStart: Double = 0.0
    for leg in legs {
        let steps = leg["steps"] as? [[String: Any]] ?? []
        for step in steps {
            let distance = (step["distance"] as? Double) ?? 0.0
            guard let m = step["maneuver"] as? [String: Any] else {
                distanceFromStart += distance
                continue
            }
            let type = ((m["type"] as? String) ?? "").lowercased()
            if type == "depart" || type == "arrive" || type == "notification" || type == "new name" || type == "continue" {
                distanceFromStart += distance
                continue
            }
            guard let location = m["location"] as? [Double], location.count >= 2 else {
                distanceFromStart += distance
                continue
            }
            let modifier = (m["modifier"] as? String) ?? ""
            let name = (step["name"] as? String) ?? ""
            maneuvers.append(RouteManeuver(
                id: name.isEmpty ? "step-\(maneuvers.count)" : "step-\(maneuvers.count)-\(name)",
                maneuverType: osrmManeuverType(type: type, modifier: modifier),
                location: CoordinatePoint(latitude: location[1], longitude: location[0]),
                distanceFromStartMeters: distanceFromStart,
                distanceToNextMeters: distance > 0 ? distance : nil,
                instructionText: osrmInstruction(type: type, modifier: modifier, name: name)
            ))
            distanceFromStart += distance
        }
    }
    maneuvers.append(RouteManeuver(
        id: "arrive", maneuverType: .arrive,
        location: geometry.last ?? CoordinatePoint(latitude: 0, longitude: 0),
        distanceFromStartMeters: (routeJson["distance"] as? Double) ?? distanceFromStart,
        distanceToNextMeters: nil,
        instructionText: "Arrive at destination"
    ))
    return maneuvers
}

private func firstOsrmStepDistance(legs: [[String: Any]]) -> Double? {
    for leg in legs {
        let steps = leg["steps"] as? [[String: Any]] ?? []
        for step in steps {
            let m = step["maneuver"] as? [String: Any]
            let type = ((m?["type"] as? String) ?? "").lowercased()
            if type != "depart" {
                if let d = step["distance"] as? Double, d > 0 { return d }
            }
        }
    }
    return nil
}

private func osrmManeuverType(type: String, modifier: String) -> RouteManeuverType {
    switch type {
    case "roundabout", "rotary": return .roundabout
    case "merge", "fork", "on ramp", "off ramp": return .merge
    case "arrive": return .arrive
    default:
        switch modifier.lowercased() {
        case "uturn": return .uturn
        case "sharp right": return .sharpRight
        case "right": return .right
        case "slight right": return .slightRight
        case "sharp left": return .sharpLeft
        case "left": return .left
        case "slight left": return .slightLeft
        default: return .straight
        }
    }
}

private func osrmInstruction(type: String, modifier: String, name: String) -> String {
    switch type {
    case "roundabout", "rotary": return "Enter roundabout"
    case "merge": return "Merge"
    case "fork":
        if modifier.lowercased().contains("left") { return "Keep left" }
        if modifier.lowercased().contains("right") { return "Keep right" }
        return "Keep to the fork"
    default:
        switch modifier.lowercased() {
        case "uturn": return "Make a U-turn"
        case "sharp right": return "Turn sharply right"
        case "right": return "Turn right"
        case "slight right": return "Slight right"
        case "sharp left": return "Turn sharply left"
        case "left": return "Turn left"
        case "slight left": return "Slight left"
        default: return name.isEmpty ? "Continue" : "Continue on \(name)"
        }
    }
}

private func osrmRouteId(request: RoutePlanRequest) -> String {
    let o = String(format: "%.5f,%.5f", request.origin.latitude, request.origin.longitude)
    let d = String(format: "%.5f,%.5f", request.destination.latitude, request.destination.longitude)
    return "osm-osrm:\(o)->\(d)"
}

private func dedupeAlternatives(_ candidates: [RouteAlternative]) -> [RouteAlternative] {
    var kept: [RouteAlternative] = []
    for c in candidates where !kept.contains(where: { areNearIdentical($0, c) }) {
        kept.append(c)
    }
    return kept
}

private func areNearIdentical(_ a: RouteAlternative, _ b: RouteAlternative) -> Bool {
    let aLen = a.normalizedPackage.summary.totalDistanceMeters
    let bLen = b.normalizedPackage.summary.totalDistanceMeters
    if aLen <= 0 || bLen <= 0 { return false }
    let lengthDelta = abs(aLen - bLen) / max(aLen, bLen)
    if lengthDelta > 0.03 { return false }
    let ag = a.normalizedPackage.geometry
    let bg = b.normalizedPackage.geometry
    if ag.isEmpty || bg.isEmpty { return false }
    return samePoint(ag.first!, bg.first!) &&
        samePoint(ag.last!, bg.last!) &&
        samePoint(ag[ag.count / 2], bg[bg.count / 2])
}

private func samePoint(_ a: CoordinatePoint, _ b: CoordinatePoint) -> Bool {
    return abs(a.latitude - b.latitude) < 1e-4 && abs(a.longitude - b.longitude) < 1e-4
}
