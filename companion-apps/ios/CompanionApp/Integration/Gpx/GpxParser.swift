import Foundation

struct GpxPoint: Equatable {
    var point: CoordinatePoint
    var label: String?
}

struct ParsedGpxRoute {
    var routeName: String?
    var points: [GpxPoint]
    var preferPointLabels: Bool
}

final class GpxParser: NSObject, XMLParserDelegate {
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

    func parser(_ parser: XMLParser, didStartElement elementName: String, namespaceURI: String?, qualifiedName qName: String?, attributes attributeDict: [String: String] = [:]) {
        currentTextBuffer = ""
        switch elementName {
        case "metadata": inMetadata = true
        case "rte": inRoute = true
        case "trk": inTrack = true
        case "rtept", "trkpt":
            guard let lat = attributeDict["lat"].flatMap(Double.init), let lon = attributeDict["lon"].flatMap(Double.init) else { return }
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
        default: break
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
        default: break
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
