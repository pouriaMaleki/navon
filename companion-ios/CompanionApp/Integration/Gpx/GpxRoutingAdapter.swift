import Foundation

struct GpxRoutingAdapter: RoutingProvider {
    let providerID: RouteProviderID = .gpxImport
    let isAvailableInV1: Bool = true

    func planRoute(_ request: RoutePlanRequest) async throws -> RoutePreviewModel {
        throw GpxRoutingAdapterError.fileImportRequired
    }

    func replanRoute(using session: ActiveRouteSession, riderLocation: CoordinatePoint) async throws -> RoutePreviewModel {
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
        return delta > 0 ? (.slightRight, "Bear right") : (.slightLeft, "Bear left")
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

private struct GpxPoint: Equatable {
    var point: CoordinatePoint
    var label: String?
}

private struct ParsedGpxRoute {
    var routeName: String?
    var points: [GpxPoint]
    var preferPointLabels: Bool
}

private final class GpxParser: NSObject, XMLParserDelegate {
    private let parser: XMLParser
    private var routePoints: [GpxPoint] = []
    private var trackPoints: [GpxPoint] = []
    private var metadataName: String?
    private var routeName: String?
    private var trackName: String?
    private var currentPoint: GpxPoint?
    private var currentPointKind: String?
    private var currentTextBuffer = ""
    private var currentPointName: String?
    private var currentPointDesc: String?
    private var currentPointComment: String?
    private var inMetadata = false
    private var inRoute = false
    private var inTrack = false
    private var currentTextTarget: TextTarget?

    init(data: Data) {
        parser = XMLParser(data: data)
        super.init()
        parser.delegate = self
    }

    func parse() throws -> ParsedGpxRoute {
        guard parser.parse() else {
            throw GpxRoutingAdapterError.parseFailed(parser.parserError?.localizedDescription ?? "Unknown GPX parse failure")
        }

        let chosen = routePoints.count >= 2 ? dedupe(points: routePoints) : dedupe(points: trackPoints)
        guard chosen.count >= 2 else {
            throw GpxRoutingAdapterError.noUsableGeometry
        }
        return ParsedGpxRoute(
            routeName: metadataName ?? routeName ?? trackName,
            points: chosen,
            preferPointLabels: routePoints.count >= 2
        )
    }

    func parser(_ parser: XMLParser, didStartElement elementName: String, namespaceURI: String?, qualifiedName qName: String?, attributes attributeDict: [String : String] = [:]) {
        currentTextBuffer = ""
        switch elementName {
        case "metadata": inMetadata = true
        case "rte": inRoute = true
        case "trk": inTrack = true
        case "rtept", "trkpt":
            guard let lat = attributeDict["lat"].flatMap(Double.init), let lon = attributeDict["lon"].flatMap(Double.init) else {
                return
            }
            currentPoint = GpxPoint(point: CoordinatePoint(latitude: lat, longitude: lon), label: nil)
            currentPointKind = elementName
            currentPointName = nil
            currentPointDesc = nil
            currentPointComment = nil
        case "name":
            if currentPoint != nil {
                currentTextTarget = .pointName
            } else if inMetadata {
                currentTextTarget = .metadataName
            } else if inRoute {
                currentTextTarget = .routeName
            } else if inTrack {
                currentTextTarget = .trackName
            }
        case "desc":
            if currentPoint != nil { currentTextTarget = .pointDesc }
        case "cmt":
            if currentPoint != nil { currentTextTarget = .pointComment }
        default:
            break
        }
    }

    func parser(_ parser: XMLParser, foundCharacters string: String) {
        currentTextBuffer += string
    }

    func parser(_ parser: XMLParser, didEndElement elementName: String, namespaceURI: String?, qualifiedName qName: String?) {
        let text = currentTextBuffer.trimmingCharacters(in: .whitespacesAndNewlines)
        if !text.isEmpty {
            switch currentTextTarget {
            case .metadataName: metadataName = text
            case .routeName: routeName = text
            case .trackName: trackName = text
            case .pointName: currentPointName = text
            case .pointDesc: currentPointDesc = text
            case .pointComment: currentPointComment = text
            case .none: break
            }
        }
        currentTextTarget = nil
        currentTextBuffer = ""

        switch elementName {
        case "metadata": inMetadata = false
        case "rte": inRoute = false
        case "trk": inTrack = false
        case "rtept", "trkpt":
            if var point = currentPoint {
                point.label = currentPointName ?? currentPointDesc ?? currentPointComment
                if currentPointKind == "rtept" {
                    routePoints.append(point)
                } else {
                    trackPoints.append(point)
                }
            }
            currentPoint = nil
            currentPointKind = nil
            currentPointName = nil
            currentPointDesc = nil
            currentPointComment = nil
        default:
            break
        }
    }

    private func dedupe(points: [GpxPoint]) -> [GpxPoint] {
        var output: [GpxPoint] = []
        for point in points where output.last?.point != point.point {
            output.append(point)
        }
        return output
    }

    private enum TextTarget {
        case metadataName
        case routeName
        case trackName
        case pointName
        case pointDesc
        case pointComment
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
