import Foundation

/// Pure utility functions for share-import processing: URL canonicalization,
/// coordinate extraction, polyline decoding, HTML scraping, GPX generation,
/// and Garmin course handling. Stateless — no AppModel dependency.
enum ShareImportUtilities {

    // MARK: - Coordinate extraction

    static func extractCoordinateSequence(fromGarminJSONObject object: Any) -> [CoordinatePoint] {
        var coordinates: [CoordinatePoint] = []
        collectCoordinateSequence(from: object, parentKey: nil, into: &coordinates)
        return coordinates
    }

    static func collectCoordinateSequence(from value: Any, parentKey: String?, into coordinates: inout [CoordinatePoint]) {
        if let dictionary = value as? [String: Any] {
            if let point = coordinate(fromJSONObjectDictionary: dictionary) {
                appendCoordinate(point, into: &coordinates)
            }
            for (key, nestedValue) in dictionary {
                if let string = nestedValue as? String,
                   let polylinePoints = decodedPolylineCoordinates(from: string, key: key),
                   polylinePoints.count >= 2 {
                    for point in polylinePoints {
                        appendCoordinate(point, into: &coordinates)
                    }
                    continue
                }
                collectCoordinateSequence(from: nestedValue, parentKey: key, into: &coordinates)
            }
            return
        }
        if let array = value as? [Any] {
            if let point = coordinate(fromJSONArray: array, parentKey: parentKey) {
                appendCoordinate(point, into: &coordinates)
                return
            }
            for nestedValue in array {
                collectCoordinateSequence(from: nestedValue, parentKey: parentKey, into: &coordinates)
            }
        }
    }

    static func coordinate(fromJSONObjectDictionary dictionary: [String: Any]) -> CoordinatePoint? {
        let latitudeKeys = ["latitude", "lat", "startlatitude", "startlat", "locationlatitude", "courselatitude"]
        let longitudeKeys = ["longitude", "lon", "lng", "startlongitude", "startlon", "startlng", "locationlongitude", "courselongitude"]
        let normalized = Dictionary(uniqueKeysWithValues: dictionary.map { ($0.key.lowercased(), $0.value) })
        guard let latitude = latitudeKeys.compactMap({ doubleValue(from: normalized[$0]) }).first,
              let longitude = longitudeKeys.compactMap({ doubleValue(from: normalized[$0]) }).first,
              (-90.0...90.0).contains(latitude), (-180.0...180.0).contains(longitude) else { return nil }
        return CoordinatePoint(latitude: latitude, longitude: longitude)
    }

    static func coordinate(fromJSONArray array: [Any], parentKey: String?) -> CoordinatePoint? {
        guard array.count >= 2, let first = doubleValue(from: array[0]), let second = doubleValue(from: array[1]) else { return nil }
        let key = parentKey?.lowercased() ?? ""
        let preferGeoJSON = key.contains("coord") || key.contains("point") || key.contains("track") || key.contains("path")
        let lonLat = CoordinatePoint(latitude: second, longitude: first)
        if preferGeoJSON, (-90.0...90.0).contains(lonLat.latitude), (-180.0...180.0).contains(lonLat.longitude) { return lonLat }
        let latLon = CoordinatePoint(latitude: first, longitude: second)
        if (-90.0...90.0).contains(latLon.latitude), (-180.0...180.0).contains(latLon.longitude) { return latLon }
        if (-90.0...90.0).contains(lonLat.latitude), (-180.0...180.0).contains(lonLat.longitude) { return lonLat }
        return nil
    }

    static func decodedPolylineCoordinates(from value: String, key: String) -> [CoordinatePoint]? {
        let normalizedKey = key.lowercased()
        guard normalizedKey.contains("polyline") || normalizedKey.contains("encoded") || normalizedKey.contains("shape") else { return nil }
        let decoded = decodePolyline(value)
        return decoded.count >= 2 ? decoded : nil
    }

    static func decodePolyline(_ encoded: String) -> [CoordinatePoint] {
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
            points.append(CoordinatePoint(latitude: Double(latitude) / 100_000.0, longitude: Double(longitude) / 100_000.0))
        }
        return points
    }

    static func appendCoordinate(_ point: CoordinatePoint, into coordinates: inout [CoordinatePoint]) {
        if coordinates.last != point { coordinates.append(point) }
    }

    static func doubleValue(from any: Any?) -> Double? {
        switch any {
        case let value as Double: return value
        case let value as NSNumber: return value.doubleValue
        case let value as String: return Double(value)
        default: return nil
        }
    }

    static func garminTitle(fromJSONObject object: Any) -> String? {
        if let dictionary = object as? [String: Any] {
            let preferredKeys = ["courseName", "displayName", "title", "name"]
            for key in preferredKeys {
                if let value = dictionary[key] as? String, !value.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty { return value }
            }
            for value in dictionary.values {
                if let title = garminTitle(fromJSONObject: value) { return title }
            }
        } else if let array = object as? [Any] {
            for value in array {
                if let title = garminTitle(fromJSONObject: value) { return title }
            }
        }
        return nil
    }

    static func jsonShapeSummary(for object: Any) -> String {
        if let dictionary = object as? [String: Any] { return "dict[\(dictionary.keys.sorted().prefix(8).joined(separator: ","))]" }
        if let array = object as? [Any] { return "array[count=\(array.count)]" }
        return String(describing: type(of: object))
    }

    static func debugSnippet(for value: String) -> String {
        let collapsed = value.replacingOccurrences(of: "\\s+", with: " ", options: .regularExpression)
        return String(collapsed.prefix(240))
    }

    // MARK: - GPX

    static func buildGPXDocument(name: String, coordinates: [CoordinatePoint]) -> String {
        let points = coordinates.map { String(format: "<trkpt lat=\"%.6f\" lon=\"%.6f\"></trkpt>", $0.latitude, $0.longitude) }.joined(separator: "\n")
        return """
        <?xml version="1.0" encoding="UTF-8"?>
        <gpx version="1.1" creator="Navon" xmlns="http://www.topografix.com/GPX/1/1">
          <trk><name>\(escapeXML(name))</name><trkseg>\(points)</trkseg></trk>
        </gpx>
        """
    }

    static func sanitizeImportFileName(_ value: String) -> String {
        value.replacingOccurrences(of: "/", with: "-")
            .replacingOccurrences(of: ":", with: "-")
            .replacingOccurrences(of: "\n", with: " ")
            .trimmingCharacters(in: .whitespacesAndNewlines)
    }

    static func escapeXML(_ value: String) -> String {
        value.replacingOccurrences(of: "&", with: "&amp;")
            .replacingOccurrences(of: "\"", with: "&quot;")
            .replacingOccurrences(of: "'", with: "&apos;")
            .replacingOccurrences(of: "<", with: "&lt;")
            .replacingOccurrences(of: ">", with: "&gt;")
    }

    // MARK: - URL handling

    static func googleMapsQueryTitle(from url: URL) -> String? {
        guard let components = URLComponents(url: url, resolvingAgainstBaseURL: false) else { return nil }
        for name in ["q", "query", "destination", "daddr"] {
            if let value = components.queryItems?.first(where: { $0.name.lowercased() == name })?.value {
                let normalized = value.replacingOccurrences(of: "+", with: " ").trimmingCharacters(in: .whitespacesAndNewlines)
                if !normalized.isEmpty, extractCoordinate(from: normalized) == nil { return normalized }
            }
        }
        return nil
    }

    static func extractURL(from value: String) -> String? {
        value.components(separatedBy: .newlines)
            .map { $0.trimmingCharacters(in: .whitespacesAndNewlines) }
            .first(where: { $0.starts(with: "http://") || $0.starts(with: "https://") })
    }

    static func isGoogleMapsURL(_ value: String) -> Bool {
        guard let host = URL(string: value)?.host?.lowercased() else { return false }
        return host.contains("google.") || host == "maps.app.goo.gl" || host == "goo.gl"
    }

    static func isSupportedSharedURL(_ url: URL) -> Bool {
        guard let host = url.host?.lowercased() else { return extractCoordinate(from: url) != nil }
        return isGoogleMapsURL(url.absoluteString) || host.contains("openstreetmap.org") || host.contains("garmin.com") || extractCoordinate(from: url) != nil
    }

    static func canonicalSharedURL(_ url: URL) -> URL {
        guard let host = url.host?.lowercased() else { return url }
        if host == "consent.google.com",
           let components = URLComponents(url: url, resolvingAgainstBaseURL: false),
           let continueValue = components.queryItems?.first(where: { $0.name == "continue" })?.value,
           let decoded = continueValue.removingPercentEncoding,
           let continueURL = URL(string: decoded) { return continueURL }
        if host == "connect.garmin.com", url.path.hasPrefix("/app/course/"),
           var components = URLComponents(url: url, resolvingAgainstBaseURL: false) {
            components.path = url.path.replacingOccurrences(of: "/app/course/", with: "/modern/course/")
            return components.url ?? url
        }
        return url
    }

    static func expandedSharedURL(for url: URL) async -> URL? {
        guard shouldExpandURL(url) else { return nil }
        var request = URLRequest(url: url)
        request.timeoutInterval = 8
        request.cachePolicy = .reloadIgnoringLocalCacheData
        request.setValue("text/html,application/xhtml+xml,application/xml;q=0.9,*/*;q=0.8", forHTTPHeaderField: "Accept")
        request.setValue("en-US,en;q=0.9", forHTTPHeaderField: "Accept-Language")
        request.setValue(safariUserAgent, forHTTPHeaderField: "User-Agent")
        do {
            let (_, response) = try await URLSession.shared.data(for: request)
            return response.url
        } catch { return nil }
    }

    static func shouldExpandURL(_ url: URL) -> Bool {
        guard let host = url.host?.lowercased() else { return false }
        return host == "maps.app.goo.gl" || host == "goo.gl" || host == "consent.google.com"
    }

    static func remotePageSummary(for url: URL) async -> RemotePageSummary? {
        guard shouldInspectRemotePage(url) else { return nil }
        var request = URLRequest(url: url)
        request.timeoutInterval = 8
        request.cachePolicy = .reloadIgnoringLocalCacheData
        request.setValue("text/html,application/xhtml+xml,application/xml;q=0.9,*/*;q=0.8", forHTTPHeaderField: "Accept")
        request.setValue("en-US,en;q=0.9", forHTTPHeaderField: "Accept-Language")
        request.setValue(safariUserAgent, forHTTPHeaderField: "User-Agent")
        do {
            let (data, response) = try await URLSession.shared.data(for: request)
            let finalURL = response.url ?? url
            let html = decodeHTMLSnippet(data)
            let garminFallback = garminCourseSummary(fromHTML: html)
            let pageTitle = sanitizedPageTitle(extractPageTitle(fromHTML: html) ?? garminFallback?.title)
            let coordinate = extractCoordinate(from: finalURL) ?? extractCoordinate(fromHTML: html) ?? garminFallback?.coordinate
            guard pageTitle != nil || coordinate != nil || finalURL.absoluteString != url.absoluteString else { return nil }
            return RemotePageSummary(finalURL: finalURL, pageTitle: pageTitle, coordinate: coordinate)
        } catch { return nil }
    }

    static func shouldInspectRemotePage(_ url: URL) -> Bool {
        guard let host = url.host?.lowercased() else { return false }
        return host.contains("garmin.com") || host.contains("openstreetmap.org")
    }

    static func decodeHTMLSnippet(_ data: Data) -> String {
        let snippet = Data(data.prefix(256_000))
        if let html = String(data: snippet, encoding: .utf8) { return html }
        if let html = String(data: snippet, encoding: .isoLatin1) { return html }
        return ""
    }

    static func extractPageTitle(fromHTML html: String) -> String? {
        if let ogTitle = firstMatch(in: html, pattern: "<meta[^>]+property=[\"']og:title[\"'][^>]+content=[\"']([^\"']+)[\"'][^>]*>") { return ogTitle }
        if let twitterTitle = firstMatch(in: html, pattern: "<meta[^>]+name=[\"']twitter:title[\"'][^>]+content=[\"']([^\"']+)[\"'][^>]*>") { return twitterTitle }
        if let metaTitle = firstMatch(in: html, pattern: "<meta[^>]+name=[\"']title[\"'][^>]+content=[\"']([^\"']+)[\"'][^>]*>") { return metaTitle }
        if let jsonLdTitle = firstMatch(in: html, pattern: "\"name\"\\s*:\\s*\"([^\"]+)\"") { return jsonLdTitle }
        return firstMatch(in: html, pattern: "<title[^>]*>(.*?)</title>")
    }

    static var safariUserAgent: String {
        "Mozilla/5.0 (iPhone; CPU iPhone OS 17_0 like Mac OS X) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/17.0 Mobile/15E148 Safari/604.1"
    }

    static func garminCourseSummary(fromHTML html: String) -> (title: String?, coordinate: CoordinatePoint?)? {
        let title = firstMatch(in: html, pattern: "\"courseName\"\\s*:\\s*\"([^\"]+)\"")
            ?? firstMatch(in: html, pattern: "\"displayName\"\\s*:\\s*\"([^\"]+)\"")
            ?? firstMatch(in: html, pattern: "\"name\"\\s*:\\s*\"([^\"]+)\"")
        let coordinate = extractNamedCoordinatePair(from: html, latitudeNames: ["startLatitude", "startLat", "start_location_lat", "courseLatitude", "beginLatitude", "locationLatitude"], longitudeNames: ["startLongitude", "startLng", "startLon", "start_location_lng", "courseLongitude", "beginLongitude", "locationLongitude"])
            ?? extractCoordinateFromCoordinateArray(in: html)
        guard title != nil || coordinate != nil else { return nil }
        return (title: title, coordinate: coordinate)
    }

    static func sanitizedPageTitle(_ rawTitle: String?) -> String? {
        guard let rawTitle else { return nil }
        var title = rawTitle
            .replacingOccurrences(of: "&amp;", with: "&")
            .replacingOccurrences(of: "&quot;", with: "\"")
            .replacingOccurrences(of: "&#39;", with: "'")
            .replacingOccurrences(of: "\n", with: " ")
            .trimmingCharacters(in: .whitespacesAndNewlines)
        for suffix in [" - Google Maps", " | Google Maps", " - OpenStreetMap", " | OpenStreetMap", " - Garmin Connect", " | Garmin Connect"] {
            if title.hasSuffix(suffix) { title.removeLast(suffix.count) }
        }
        if title.isEmpty || extractCoordinate(from: title) != nil { return nil }
        let lower = title.lowercased()
        if lower == "google maps" || lower == "openstreetmap" || lower == "garmin connect" { return nil }
        return title
    }

    // MARK: - Coordinate extraction from URLs and text

    static func extractCoordinate(from url: URL) -> CoordinatePoint? {
        guard let components = URLComponents(url: url, resolvingAgainstBaseURL: false) else { return nil }
        let host = url.host?.lowercased() ?? ""
        if let coordinate = extractCoordinate(from: components.queryItems) { return coordinate }
        if let fragment = components.fragment {
            if let coordinate = extractCoordinate(fromMapFragment: fragment) { return coordinate }
            if let coordinate = extractCoordinate(from: fragment) { return coordinate }
        }
        if host.contains("google."), let coordinate = extractCoordinate(fromGooglePath: url.path) { return coordinate }
        if !host.contains("google.") && host != "consent.google.com" {
            if let coordinate = extractCoordinate(from: url.absoluteString.removingPercentEncoding ?? url.absoluteString) { return coordinate }
        }
        return nil
    }

    static func extractCoordinate(fromGooglePath path: String) -> CoordinatePoint? {
        guard let groups = firstMatchGroups(in: path, pattern: "@(-?\\d{1,3}\\.\\d+),(-?\\d{1,3}\\.\\d+)"),
              groups.count == 2, let latitude = Double(groups[0]), let longitude = Double(groups[1]),
              (-90.0...90.0).contains(latitude), (-180.0...180.0).contains(longitude) else { return nil }
        return CoordinatePoint(latitude: latitude, longitude: longitude)
    }

    static func extractCoordinate(from queryItems: [URLQueryItem]?) -> CoordinatePoint? {
        guard let queryItems else { return nil }
        var namedValues: [String: String] = [:]
        for item in queryItems where namedValues[item.name.lowercased()] == nil { namedValues[item.name.lowercased()] = item.value ?? "" }
        let latitudeKeys = ["lat", "latitude", "mlat"]
        let longitudeKeys = ["lon", "lng", "longitude", "mlon"]
        if let latStr = latitudeKeys.compactMap({ namedValues[$0] }).first(where: { !$0.isEmpty }),
           let lonStr = longitudeKeys.compactMap({ namedValues[$0] }).first(where: { !$0.isEmpty }),
           let latitude = Double(latStr), let longitude = Double(lonStr),
           (-90.0...90.0).contains(latitude), (-180.0...180.0).contains(longitude) {
            return CoordinatePoint(latitude: latitude, longitude: longitude)
        }
        for name in ["ll", "sll", "center", "destination", "daddr", "near", "q", "query"] {
            if let value = namedValues[name]?.replacingOccurrences(of: "loc:", with: ""), let coordinate = extractCoordinate(from: value) { return coordinate }
        }
        return nil
    }

    static func extractCoordinate(fromMapFragment fragment: String) -> CoordinatePoint? {
        guard let zoomlessMatch = firstMatchGroups(in: fragment, pattern: "map=\\d+(?:\\.\\d+)?/(-?\\d{1,3}\\.\\d+)/(-?\\d{1,3}\\.\\d+)"),
              zoomlessMatch.count == 2, let latitude = Double(zoomlessMatch[0]), let longitude = Double(zoomlessMatch[1]),
              (-90.0...90.0).contains(latitude), (-180.0...180.0).contains(longitude) else { return nil }
        return CoordinatePoint(latitude: latitude, longitude: longitude)
    }

    static func extractCoordinate(fromHTML html: String) -> CoordinatePoint? {
        if let named = extractNamedCoordinatePair(from: html, latitudeNames: ["latitude", "lat", "startLatitude", "endLatitude", "centerLatitude"], longitudeNames: ["longitude", "lng", "lon", "startLongitude", "endLongitude", "centerLongitude"]) { return named }
        return extractCoordinate(from: html)
    }

    static func extractCoordinateFromCoordinateArray(in value: String) -> CoordinatePoint? {
        guard let groups = firstMatchGroups(in: value, pattern: "\"coordinates\"\\s*:\\s*\\[\\s*(-?\\d{1,3}\\.\\d+)\\s*,\\s*(-?\\d{1,3}\\.\\d+)\\s*\\]"),
              groups.count == 2, let longitude = Double(groups[0]), let latitude = Double(groups[1]),
              (-90.0...90.0).contains(latitude), (-180.0...180.0).contains(longitude) else { return nil }
        return CoordinatePoint(latitude: latitude, longitude: longitude)
    }

    static func extractNamedCoordinatePair(from value: String, latitudeNames: [String], longitudeNames: [String]) -> CoordinatePoint? {
        for latitudeName in latitudeNames {
            for longitudeName in longitudeNames {
                let pattern = "\(latitudeName)[\"'=:\\s>]+(-?\\d{1,3}\\.\\d+).*?\(longitudeName)[\"'=:\\s>]+(-?\\d{1,3}\\.\\d+)"
                if let groups = firstMatchGroups(in: value, pattern: pattern), groups.count == 2,
                   let latitude = Double(groups[0]), let longitude = Double(groups[1]),
                   (-90.0...90.0).contains(latitude), (-180.0...180.0).contains(longitude) {
                    return CoordinatePoint(latitude: latitude, longitude: longitude)
                }
            }
        }
        return nil
    }

    static func extractCoordinate(from value: String) -> CoordinatePoint? {
        let pattern = try? NSRegularExpression(pattern: "(-?\\d{1,3}\\.\\d+)[,\\s/]+(-?\\d{1,3}\\.\\d+)")
        guard let match = pattern?.firstMatch(in: value, range: NSRange(location: 0, length: value.utf16.count)),
              let latRange = Range(match.range(at: 1), in: value), let lonRange = Range(match.range(at: 2), in: value),
              let latitude = Double(value[latRange]), let longitude = Double(value[lonRange]),
              (-90.0...90.0).contains(latitude), (-180.0...180.0).contains(longitude) else { return nil }
        return CoordinatePoint(latitude: latitude, longitude: longitude)
    }

    // MARK: - Regex helpers

    static func firstMatch(in value: String, pattern: String) -> String? {
        guard let groups = firstMatchGroups(in: value, pattern: pattern), let first = groups.first else { return nil }
        return first
    }

    static func firstMatchGroups(in value: String, pattern: String) -> [String]? {
        guard let regex = try? NSRegularExpression(pattern: pattern, options: [.caseInsensitive, .dotMatchesLineSeparators]) else { return nil }
        let fullRange = NSRange(location: 0, length: value.utf16.count)
        guard let match = regex.firstMatch(in: value, range: fullRange), match.numberOfRanges > 1 else { return nil }
        return (1..<match.numberOfRanges).compactMap { index in
            guard let range = Range(match.range(at: index), in: value) else { return nil }
            return String(value[range]).trimmingCharacters(in: .whitespacesAndNewlines)
        }
    }
}

// MARK: - Shared types

struct RemotePageSummary {
    let finalURL: URL
    let pageTitle: String?
    let coordinate: CoordinatePoint?
}
