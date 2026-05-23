import Foundation

struct GpxRoutingAdapter: RoutingProvider {
    let providerID: RouteProviderID = .gpxImport
    let isAvailableInV1: Bool = true

    func planRoute(_ request: RoutePlanRequest) async throws -> RoutePreviewModel {
        throw GpxRoutingAdapterError.fileImportRequired
    }

    func replanRoute(
        using session: ActiveRouteSession,
        riderLocation: CoordinatePoint,
        rerouteContext: RerouteContext?
    ) async throws -> RoutePreviewModel {
        _ = session
        _ = riderLocation
        _ = rerouteContext
        print("[reroute_heading] provider=gpxImport reason=provider_noop")
        throw GpxRoutingAdapterError.rerouteNotSupported
    }

    func normalizePreview(_ preview: RoutePreviewModel, request: RoutePlanRequest) throws -> NormalizedRoutePackage {
        guard let selected = preview.selectedAlternative else {
            throw GpxRoutingAdapterError.noImportedRoute
        }
        return selected.normalizedPackage
    }

    func importFile(named fileName: String, data: Data, revision: Int = 1) throws -> RoutePreviewModel {
        let parser = GpxParser(data: data)
        let parsed = try parser.parse()
        let routeName = parsed.routeName ?? fileName.replacingOccurrences(of: ".gpx", with: "", options: [.caseInsensitive])
        let routeID = slugify(routeName)
        let geometry = parsed.points.map(\.point)
        let cumulative = cumulativeDistances(for: geometry)
        let maneuvers = buildManeuvers(from: parsed.points, cumulativeDistances: cumulative, preferPointLabels: parsed.preferPointLabels)
        let totalDistance = cumulative.last ?? 0
        let package = NormalizedRoutePackage(
            version: .current,
            routeIdentifier: routeID,
            revision: revision,
            geometry: geometry,
            maneuvers: maneuvers,
            summary: RouteSummary(
                totalDistanceMeters: totalDistance,
                estimatedDurationSeconds: max(Int((totalDistance / 5.0).rounded()), 60),
                startLabel: parsed.points.first?.label,
                destinationLabel: parsed.points.last?.label ?? routeName
            ),
            provenance: RouteProvenance(
                providerID: .gpxImport,
                sourceReference: fileName,
                generatedAtUnixMs: UInt64(Date().timeIntervalSince1970 * 1000)
            )
        )

        let alternative = RouteAlternative(
            id: UUID(),
            title: routeName,
            subtitle: "Imported GPX route",
            distanceMeters: Int(totalDistance.rounded()),
            durationSeconds: package.summary.estimatedDurationSeconds,
            normalizedPackage: package
        )
        return RoutePreviewModel(
            alternatives: [alternative],
            selectedAlternativeID: alternative.id,
            routeIdentifier: package.routeIdentifier,
            routeRevision: package.revision,
            planningNotice: "Imported \(fileName)"
        )
    }

    private func cumulativeDistances(for geometry: [CoordinatePoint]) -> [Double] {
        var cumulative = [0.0]
        for (start, end) in zip(geometry, geometry.dropFirst()) {
            cumulative.append((cumulative.last ?? 0) + approximateDistanceMeters(from: start, to: end))
        }
        return cumulative
    }

    private func buildManeuvers(
        from points: [GpxPoint],
        cumulativeDistances: [Double],
        preferPointLabels: Bool
    ) -> [RouteManeuver] {
        guard let first = points.first, let last = points.last else {
            return []
        }

        var maneuvers = [
            RouteManeuver(
                id: "depart",
                maneuverType: .depart,
                location: first.point,
                distanceFromStartMeters: 0,
                distanceToNextMeters: cumulativeDistances.dropFirst().first,
                instructionText: "Start riding"
            )
        ]

        for index in 1..<(points.count - 1) {
            let pointLabel = points[index].label
            let turn = classifyTurn(
                previous: points[index - 1].point,
                current: points[index].point,
                next: points[index + 1].point
            )
            if turn == nil && !(preferPointLabels && pointLabel != nil) {
                continue
            }

            let maneuverType = turn?.type ?? .straight
            let instructionText = pointLabel ?? turn?.instruction
            let distanceToNext = index + 1 < cumulativeDistances.count
                ? cumulativeDistances[index + 1] - cumulativeDistances[index]
                : nil
            maneuvers.append(
                RouteManeuver(
                    id: "step-\(index)",
                    maneuverType: maneuverType,
                    location: points[index].point,
                    distanceFromStartMeters: cumulativeDistances[index],
                    distanceToNextMeters: distanceToNext,
                    instructionText: instructionText
                )
            )
        }

        maneuvers.append(
            RouteManeuver(
                id: "arrive",
                maneuverType: .arrive,
                location: last.point,
                distanceFromStartMeters: cumulativeDistances.last ?? 0,
                distanceToNextMeters: nil,
                instructionText: "Arrive at destination"
            )
        )
        return maneuvers
    }

    private func classifyTurn(
        previous: CoordinatePoint,
        current: CoordinatePoint,
        next: CoordinatePoint
    ) -> (type: RouteManeuverType, instruction: String)? {
        let delta = turnDeltaDegrees(previous: previous, current: current, next: next)
        let magnitude = abs(delta)
        guard magnitude >= 25 else { return nil }
        if magnitude >= 170 { return (.uturn, "Make a U-turn") }
        if magnitude >= 110 {
            return delta > 0 ? (.sharpRight, "Turn sharply right") : (.sharpLeft, "Turn sharply left")
        }
        if magnitude >= 50 {
            return delta > 0 ? (.right, "Turn right") : (.left, "Turn left")
        }
        return delta > 0 ? (.slightRight, "Slight right") : (.slightLeft, "Slight left")
    }

    private func turnDeltaDegrees(previous: CoordinatePoint, current: CoordinatePoint, next: CoordinatePoint) -> Double {
        let incoming = bearingDegrees(from: previous, to: current)
        let outgoing = bearingDegrees(from: current, to: next)
        var delta = outgoing - incoming
        while delta <= -180 { delta += 360 }
        while delta > 180 { delta -= 360 }
        return delta
    }

    private func bearingDegrees(from start: CoordinatePoint, to end: CoordinatePoint) -> Double {
        let latMeters = (end.latitude - start.latitude) * 111_320.0
        let lonMeters = (end.longitude - start.longitude) * cos(((start.latitude + end.latitude) / 2.0) * .pi / 180.0) * 111_320.0
        return atan2(lonMeters, latMeters) * 180.0 / .pi
    }

    private func approximateDistanceMeters(from start: CoordinatePoint, to end: CoordinatePoint) -> Double {
        let latMeters = (end.latitude - start.latitude) * 111_320.0
        let lonMeters = (end.longitude - start.longitude) * cos(((start.latitude + end.latitude) / 2.0) * .pi / 180.0) * 111_320.0
        return sqrt(latMeters * latMeters + lonMeters * lonMeters)
    }

    private func slugify(_ value: String) -> String {
        var output = ""
        var previousDash = false
        for character in value.lowercased() {
            let next: Character = {
                if character.isASCII && (character.isLetter || character.isNumber) {
                    return character
                }
                return "-"
            }()
            if next == "-" {
                if previousDash { continue }
                previousDash = true
                output.append(next)
            } else {
                previousDash = false
                output.append(next)
            }
        }
        let trimmed = output.trimmingCharacters(in: CharacterSet(charactersIn: "-"))
        return trimmed.isEmpty ? "gpx-import" : trimmed
    }
}


enum GpxRoutingAdapterError: LocalizedError {
    case fileImportRequired
    case rerouteNotSupported
    case noImportedRoute
    case parseFailed(String)
    case noUsableGeometry

    var errorDescription: String? {
        switch self {
        case .fileImportRequired:
            return "Select a GPX file instead of using coordinate planning."
        case .rerouteNotSupported:
            return "Reroute is not supported for imported GPX routes yet."
        case .noImportedRoute:
            return "No imported GPX route is available."
        case .parseFailed(let detail):
            return "GPX import failed: \(detail)"
        case .noUsableGeometry:
            return "GPX did not contain a usable route or track."
        }
    }
}
