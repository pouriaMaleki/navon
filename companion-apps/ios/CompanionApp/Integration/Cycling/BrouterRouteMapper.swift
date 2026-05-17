import Foundation

/// Convert a BRouter Feature (`features[0]` from `format=geojson&timode=2`)
/// into a `RouteAlternative` ready to drop into a `RoutePreviewModel`.
/// Pure function — no I/O. Mirrors web companion's `mapBrouterToRoute.ts`
/// byte-for-byte.
func mapBrouterToAlternative(
    feature: [String: Any],
    request: RoutePlanRequest,
    revision: Int,
    profile: BrouterProfile,
    title: String
) -> RouteAlternative? {
    guard let geometry = feature["geometry"] as? [String: Any],
          let coordsRaw = geometry["coordinates"] as? [[Double]] else {
        return nil
    }
    let coords: [CoordinatePoint] = coordsRaw.compactMap { c in
        guard c.count >= 2 else { return nil }
        return CoordinatePoint(latitude: c[1], longitude: c[0])
    }
    guard coords.count >= 2 else { return nil }
    let props = feature["properties"] as? [String: Any] ?? [:]
    let distance = parseNumberLike(props["track-length"]) ?? 0.0
    let duration = max(parseNumberLike(props["total-time"]).map { Int($0) } ?? 0, 60)
    let voicehints = props["voicehints"] as? [[Any]] ?? []
    let maneuvers = buildManeuversFromVoiceHints(geometry: coords, voicehints: voicehints)
    let routePackage = NormalizedRoutePackage(
        version: RoutePackageVersion.current,
        routeIdentifier: brouterRouteId(request: request, profile: profile),
        revision: revision,
        geometry: coords,
        maneuvers: maneuvers,
        summary: RouteSummary(
            totalDistanceMeters: distance,
            estimatedDurationSeconds: duration,
            startLabel: "Current location",
            destinationLabel: "Selected destination"
        ),
        provenance: RouteProvenance(
            providerID: .osm,
            sourceReference: "BRouter \(profile.rawValue)",
            generatedAtUnixMs: UInt64(Date().timeIntervalSince1970 * 1000)
        )
    )
    let km = String(format: "%.1f", distance / 1000.0)
    let min = max(duration / 60, 1)
    return RouteAlternative(
        id: UUID(),
        title: title,
        subtitle: "\(km) km • \(min) min",
        distanceMeters: Int(distance),
        durationSeconds: duration,
        normalizedPackage: routePackage
    )
}

/// BRouter `voicehints`: [geomIdx, cmdType, exitCount, distToNext, angle].
/// Codes match the web/Android mappers byte-for-byte.
private func buildManeuversFromVoiceHints(
    geometry: [CoordinatePoint],
    voicehints: [[Any]]
) -> [RouteManeuver] {
    var maneuvers: [RouteManeuver] = []
    let firstNext = voicehints.first.flatMap { hint in (hint.count > 3 ? parseNumberLike(hint[3]) : nil) }
    maneuvers.append(RouteManeuver(
        id: "depart",
        maneuverType: .depart,
        location: geometry.first ?? CoordinatePoint(latitude: 0, longitude: 0),
        distanceFromStartMeters: 0,
        distanceToNextMeters: firstNext,
        instructionText: "Start riding"
    ))
    for (i, hint) in voicehints.enumerated() {
        guard hint.count >= 4 else { continue }
        let geomIdx = Int(parseNumberLike(hint[0]) ?? 0)
        let cmd = Int(parseNumberLike(hint[1]) ?? 0)
        let distToNext = parseNumberLike(hint[3])
        let type = cmdToManeuver(cmd)
        if type == .straight { continue }
        let cappedIdx = max(0, min(geomIdx, geometry.count - 1))
        maneuvers.append(RouteManeuver(
            id: "vh-\(i)",
            maneuverType: type,
            location: geometry[cappedIdx],
            distanceFromStartMeters: approxDistanceFromStart(geometry: geometry, targetIdx: cappedIdx),
            distanceToNextMeters: distToNext,
            instructionText: cmdInstruction(cmd)
        ))
    }
    maneuvers.append(RouteManeuver(
        id: "arrive",
        maneuverType: .arrive,
        location: geometry.last ?? CoordinatePoint(latitude: 0, longitude: 0),
        distanceFromStartMeters: polylineLengthMeters(geometry),
        distanceToNextMeters: nil,
        instructionText: "Arrive at destination"
    ))
    return maneuvers
}

private func cmdToManeuver(_ cmd: Int) -> RouteManeuverType {
    switch cmd {
    case 2: return .left
    case 3: return .slightLeft
    case 4: return .sharpLeft
    case 5: return .right
    case 6: return .slightRight
    case 7: return .sharpRight
    case 8: return .slightLeft   // keep-left ≈ slight-left
    case 9: return .slightRight  // keep-right ≈ slight-right
    case 10: return .uturn
    case 12: return .roundabout
    case 13: return .arrive
    default: return .straight
    }
}

private func cmdInstruction(_ cmd: Int) -> String {
    switch cmd {
    case 1: return "Continue"
    case 2: return "Turn left"
    case 3: return "Slight left"
    case 4: return "Turn sharply left"
    case 5: return "Turn right"
    case 6: return "Slight right"
    case 7: return "Turn sharply right"
    case 8: return "Keep left"
    case 9: return "Keep right"
    case 10: return "Make a U-turn"
    case 12: return "Enter roundabout"
    case 13: return "Arrive at destination"
    default: return "Continue"
    }
}

private func approxDistanceFromStart(geometry: [CoordinatePoint], targetIdx: Int) -> Double {
    var total = 0.0
    let cap = max(1, min(targetIdx, geometry.count - 1))
    for i in 1...cap { total += haversine(geometry[i - 1], geometry[i]) }
    return total
}

private func polylineLengthMeters(_ geometry: [CoordinatePoint]) -> Double {
    var total = 0.0
    for i in 1..<geometry.count { total += haversine(geometry[i - 1], geometry[i]) }
    return total
}

private func haversine(_ a: CoordinatePoint, _ b: CoordinatePoint) -> Double {
    let metersPerDegLat = 111_320.0
    let meanLatRad = (a.latitude + b.latitude) / 2.0 * .pi / 180.0
    let dN: Double = (b.latitude - a.latitude) * metersPerDegLat
    let dE: Double = (b.longitude - a.longitude) * cos(meanLatRad) * metersPerDegLat
    return (dN * dN + dE * dE).squareRoot()
}

private func parseNumberLike(_ value: Any?) -> Double? {
    if let d = value as? Double { return d }
    if let i = value as? Int { return Double(i) }
    if let s = value as? String { return Double(s) }
    if let n = value as? NSNumber { return n.doubleValue }
    return nil
}

private func brouterRouteId(request: RoutePlanRequest, profile: BrouterProfile) -> String {
    let o = String(format: "%.5f,%.5f", request.origin.latitude, request.origin.longitude)
    let d = String(format: "%.5f,%.5f", request.destination.latitude, request.destination.longitude)
    return "osm-brouter-\(profile.rawValue):\(o)->\(d)"
}
