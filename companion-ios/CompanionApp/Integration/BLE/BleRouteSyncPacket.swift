import Foundation

enum BleRouteSyncGattContract {
    static let serviceUUID = "8d0f3f30-7b4d-4f7c-8b24-2f8e7e4e1001"
    static let chunkWriteCharacteristicUUID = "8d0f3f30-7b4d-4f7c-8b24-2f8e7e4e1002"
    static let eventNotifyCharacteristicUUID = "8d0f3f30-7b4d-4f7c-8b24-2f8e7e4e1003"
}

struct RouteTransferChunkEnvelope: Equatable {
    var transferIdentifier: String
    var chunkIndex: Int
    var totalChunks: Int
    var checksumHex: String
    var payloadFragment: Data
}

enum BleRouteSyncPacket: Equatable {
    case chunk(RouteTransferChunkEnvelope)
    case syncMessage(RouteSyncMessage)
}

enum BleRouteSyncPacketError: Error, Equatable {
    case missingHeaderSeparator
    case invalidUtf8Header
    case missingField(String)
    case invalidField(field: String, value: String)
    case payloadLengthMismatch(expected: Int, actual: Int)
    case unsupportedPacketType(String)
    case unsupportedVersion(String)
    case invalidSyncMessage(String)
}

enum BleRouteSyncCodec {
    private static let packetVersion = "1"

    static func canonicalPayloadData(for message: RouteSyncMessage) -> Data {
        Data(canonicalPayloadString(for: message).utf8)
    }

    static func checksumHex(for data: Data) -> String {
        var hash: UInt32 = 2_166_136_261
        for byte in data {
            hash ^= UInt32(byte)
            hash &*= 16_777_619
        }
        return String(format: "%08x", hash)
    }

    static func chunkEnvelopes(
        for message: RouteSyncMessage,
        transferIdentifier: String,
        chunkSizeBytes: Int
    ) -> [RouteTransferChunkEnvelope] {
        let payload = canonicalPayloadData(for: message)
        let safeChunkSize = max(chunkSizeBytes, 1)
        let checksum = checksumHex(for: payload)
        let totalChunks = max(Int(ceil(Double(payload.count) / Double(safeChunkSize))), 1)
        return stride(from: 0, to: payload.count, by: safeChunkSize).enumerated().map { index, offset in
            let end = min(offset + safeChunkSize, payload.count)
            return RouteTransferChunkEnvelope(
                transferIdentifier: transferIdentifier,
                chunkIndex: index,
                totalChunks: totalChunks,
                checksumHex: checksum,
                payloadFragment: payload.subdata(in: offset..<end)
            )
        }
    }

    static func encode(_ packet: BleRouteSyncPacket) -> Data {
        switch packet {
        case .chunk(let chunk):
            return encodePacket(
                headers: [
                    ("v", packetVersion),
                    ("type", "chunk"),
                    ("transfer_id", chunk.transferIdentifier),
                    ("chunk_index", String(chunk.chunkIndex)),
                    ("total_chunks", String(chunk.totalChunks)),
                    ("checksum", chunk.checksumHex),
                ],
                body: chunk.payloadFragment
            )
        case .syncMessage(let message):
            return encodePacket(
                headers: [
                    ("v", packetVersion),
                    ("type", "sync_message"),
                ],
                body: canonicalPayloadData(for: message)
            )
        }
    }

    static func decode(_ payload: Data) throws -> BleRouteSyncPacket {
        let (headers, body) = try splitHeaderAndBody(payload)
        let version = try requiredField("v", in: headers)
        guard version == packetVersion else {
            throw BleRouteSyncPacketError.unsupportedVersion(version)
        }

        switch try requiredField("type", in: headers) {
        case "chunk":
            return .chunk(
                RouteTransferChunkEnvelope(
                    transferIdentifier: try requiredField("transfer_id", in: headers),
                    chunkIndex: try parseInt(try requiredField("chunk_index", in: headers), field: "chunk_index"),
                    totalChunks: try parseInt(try requiredField("total_chunks", in: headers), field: "total_chunks"),
                    checksumHex: try requiredField("checksum", in: headers),
                    payloadFragment: body
                )
            )
        case "sync_message":
            return .syncMessage(try decodeSyncMessage(body))
        case let other:
            throw BleRouteSyncPacketError.unsupportedPacketType(other)
        }
    }

    static func decodeSyncMessage(_ payload: Data) throws -> RouteSyncMessage {
        guard let text = String(data: payload, encoding: .utf8) else {
            throw BleRouteSyncPacketError.invalidSyncMessage("payload was not valid UTF-8")
        }
        return try parseSyncMessage(text)
    }

    private static func canonicalPayloadString(for message: RouteSyncMessage) -> String {
        switch message {
        case .set(let message):
            return routePayloadString(kind: "set", route: message.route)
        case .update(let message):
            return routePayloadString(kind: "update", route: message.route)
        case .clear(let message):
            return [
                "kind=clear",
                "route_id=\(message.routeIdentifier ?? "current")"
            ].joined(separator: "\n")
        case .status(let message):
            return [
                "kind=status",
                "route_id=\(message.routeIdentifier ?? "none")",
                "revision=\(message.revision.map(String.init) ?? "none")",
                "status=\(message.status.rawValue)",
                "detail=\(message.detail ?? "")"
            ].joined(separator: "\n")
        case .rerouteRequest(let message):
            return [
                "kind=reroute_request",
                "route_id=\(message.routeIdentifier)",
                String(format: "rider=%.6f,%.6f", message.riderLocation.latitude, message.riderLocation.longitude),
                "reason=\(message.reason)"
            ].joined(separator: "\n")
        }
    }

    private static func routePayloadString(kind: String, route: NormalizedRoutePackage) -> String {
        let geometry = route.geometry.map {
            String(format: "%.6f,%.6f", $0.latitude, $0.longitude)
        }.joined(separator: ";")
        let maneuvers = route.maneuvers.map { maneuver in
            [
                maneuver.id,
                maneuver.maneuverType.rawValue,
                String(format: "%.1f", maneuver.distanceFromStartMeters),
                maneuver.distanceToNextMeters.map { String(format: "%.1f", $0) } ?? "",
                String(format: "%.6f,%.6f", maneuver.location.latitude, maneuver.location.longitude),
                maneuver.instructionText ?? ""
            ].joined(separator: "|")
        }.joined(separator: ";")

        return [
            "kind=\(kind)",
            "route_id=\(route.routeIdentifier)",
            "revision=\(route.revision)",
            "version=\(route.version.major).\(route.version.minor)",
            String(
                format: "summary=%.1f|%d|%@|%@",
                route.summary.totalDistanceMeters,
                route.summary.estimatedDurationSeconds,
                route.summary.startLabel ?? "",
                route.summary.destinationLabel ?? ""
            ),
            "geometry=\(geometry)",
            "maneuvers=\(maneuvers)",
            "provenance=\(route.provenance.providerID.rawValue)|\(route.provenance.sourceReference ?? "")|\(route.provenance.generatedAtUnixMs)"
        ].joined(separator: "\n")
    }

    private static func parseSyncMessage(_ payload: String) throws -> RouteSyncMessage {
        let fields = Dictionary(uniqueKeysWithValues: payload.split(separator: "\n").compactMap { line in
            guard let separator = line.firstIndex(of: "=") else { return nil }
            let key = String(line[..<separator]).trimmingCharacters(in: .whitespaces)
            let value = String(line[line.index(after: separator)...])
            return (key, value)
        })
        let kind = try requiredField("kind", in: fields)
        switch kind {
        case "set":
            return .set(RouteSetMessage(route: try parseRoutePackage(fields)))
        case "update":
            let route = try parseRoutePackage(fields)
            return .update(
                RouteUpdateMessage(
                    routeIdentifier: route.routeIdentifier,
                    revision: route.revision,
                    route: route
                )
            )
        case "clear":
            return .clear(RouteClearMessage(routeIdentifier: optionalStringField("route_id", in: fields)))
        case "status":
            return .status(
                RouteStatusMessage(
                    routeIdentifier: optionalStringField("route_id", in: fields),
                    revision: try optionalIntField("revision", in: fields),
                    status: try parseStatus(try requiredField("status", in: fields)),
                    detail: optionalStringField("detail", in: fields)
                )
            )
        case "reroute_request":
            return .rerouteRequest(
                RouteRerouteRequestMessage(
                    routeIdentifier: optionalStringField("route_id", in: fields) ?? "none",
                    riderLocation: try parseCoordinatePoint(try requiredField("rider", in: fields)),
                    reason: try requiredField("reason", in: fields)
                )
            )
        default:
            throw BleRouteSyncPacketError.invalidSyncMessage("Unsupported kind \(kind)")
        }
    }

    private static func parseRoutePackage(_ fields: [String: String]) throws -> NormalizedRoutePackage {
        let version = try requiredField("version", in: fields)
        let versionParts = version.split(separator: ".", maxSplits: 1, omittingEmptySubsequences: false)
        guard versionParts.count == 2 else {
            throw BleRouteSyncPacketError.invalidField(field: "version", value: version)
        }
        let summary = try parseSummary(try requiredField("summary", in: fields))
        let provenance = try parseProvenance(try requiredField("provenance", in: fields))
        return NormalizedRoutePackage(
            version: RoutePackageVersion(
                major: UInt16(try parseInt(String(versionParts[0]), field: "version.major")),
                minor: UInt16(try parseInt(String(versionParts[1]), field: "version.minor"))
            ),
            routeIdentifier: try requiredField("route_id", in: fields),
            revision: try parseInt(try requiredField("revision", in: fields), field: "revision"),
            geometry: try parseGeometry(try requiredField("geometry", in: fields)),
            maneuvers: try parseManeuvers(try requiredField("maneuvers", in: fields)),
            summary: summary,
            provenance: provenance
        )
    }

    private static func parseSummary(_ value: String) throws -> RouteSummary {
        let parts = value.split(separator: "|", maxSplits: 3, omittingEmptySubsequences: false).map(String.init)
        guard parts.count == 4 else {
            throw BleRouteSyncPacketError.invalidField(field: "summary", value: value)
        }
        return RouteSummary(
            totalDistanceMeters: try parseDouble(parts[0], field: "summary.totalDistanceMeters"),
            estimatedDurationSeconds: try parseInt(parts[1], field: "summary.estimatedDurationSeconds"),
            startLabel: nonEmpty(parts[2]),
            destinationLabel: nonEmpty(parts[3])
        )
    }

    private static func parseGeometry(_ value: String) throws -> [CoordinatePoint] {
        guard !value.isEmpty else { return [] }
        return try value.split(separator: ";", omittingEmptySubsequences: false).map {
            try parseCoordinatePoint(String($0))
        }
    }

    private static func parseManeuvers(_ value: String) throws -> [RouteManeuver] {
        guard !value.isEmpty else { return [] }
        return try value.split(separator: ";", omittingEmptySubsequences: false).map { entry in
            let parts = entry.split(separator: "|", maxSplits: 5, omittingEmptySubsequences: false).map(String.init)
            let distanceFromStart = try parseDouble(parts[safe: 2] ?? "", field: "maneuver.distanceFromStartMeters")
            let tuple: (Double?, String, String)
            switch parts.count {
            case 5:
                tuple = (nil, parts[3], parts[4])
            case 6:
                tuple = (
                    nonEmpty(parts[3]).map { try parseDouble($0, field: "maneuver.distanceToNextMeters") },
                    parts[4],
                    parts[5]
                )
            default:
                throw BleRouteSyncPacketError.invalidField(field: "maneuvers", value: String(entry))
            }
            return RouteManeuver(
                id: parts[safe: 0] ?? "",
                maneuverType: try parseManeuverType(parts[safe: 1] ?? ""),
                location: try parseCoordinatePoint(tuple.1),
                distanceFromStartMeters: distanceFromStart,
                distanceToNextMeters: tuple.0,
                instructionText: nonEmpty(tuple.2)
            )
        }
    }

    private static func parseProvenance(_ value: String) throws -> RouteProvenance {
        let parts = value.split(separator: "|", maxSplits: 2, omittingEmptySubsequences: false).map(String.init)
        guard parts.count == 3 else {
            throw BleRouteSyncPacketError.invalidField(field: "provenance", value: value)
        }
        return RouteProvenance(
            providerID: parseProvider(parts[0]),
            sourceReference: nonEmpty(parts[1]),
            generatedAtUnixMs: UInt64(try parseInt(parts[2], field: "provenance.generatedAtUnixMs"))
        )
    }

    private static func parseCoordinatePoint(_ value: String) throws -> CoordinatePoint {
        let parts = value.split(separator: ",", maxSplits: 1, omittingEmptySubsequences: false).map(String.init)
        guard parts.count == 2 else {
            throw BleRouteSyncPacketError.invalidField(field: "coordinate", value: value)
        }
        return CoordinatePoint(
            latitude: try parseDouble(parts[0], field: "coordinate.latitude"),
            longitude: try parseDouble(parts[1], field: "coordinate.longitude")
        )
    }

    private static func parseProvider(_ value: String) -> RouteProviderID {
        switch value.trimmingCharacters(in: .whitespacesAndNewlines) {
        case "hsl", "hsl_digitransit": return .hsl
        case "google_ingest", "googleIngest": return .googleIngest
        case "osm": return .osm
        case "gpx": return .gpxImport
        case "fit": return .fitImport
        case "tcx": return .tcxImport
        case "garmin_api", "garminApi": return .garminApi
        case "garmin_file", "garminFile": return .garminFile
        default: return .osm
        }
    }

    private static func parseManeuverType(_ value: String) throws -> RouteManeuverType {
        switch value.trimmingCharacters(in: .whitespacesAndNewlines) {
        case "depart": return .depart
        case "straight": return .straight
        case "slight_left", "slightLeft": return .slightLeft
        case "left": return .left
        case "sharp_left", "sharpLeft": return .sharpLeft
        case "slight_right", "slightRight": return .slightRight
        case "right": return .right
        case "sharp_right", "sharpRight": return .sharpRight
        case "uturn", "u_turn": return .uturn
        case "roundabout": return .roundabout
        case "merge": return .merge
        case "ramp": return .ramp
        case "arrive": return .arrive
        default:
            throw BleRouteSyncPacketError.invalidField(field: "maneuverType", value: value)
        }
    }

    private static func parseStatus(_ value: String) throws -> RouteSyncStatusCode {
        switch value.trimmingCharacters(in: .whitespacesAndNewlines) {
        case "accepted": return .accepted
        case "applying": return .applying
        case "active": return .active
        case "cleared": return .cleared
        case "rejected": return .rejected
        case "retryable_failure", "retryableFailure": return .retryableFailure
        case "fatal_failure", "fatalFailure": return .fatalFailure
        default:
            throw BleRouteSyncPacketError.invalidField(field: "status", value: value)
        }
    }

    private static func encodePacket(headers: [(String, String)], body: Data) -> Data {
        var encoded = headers.map { "\($0.0)=\($0.1)" }.joined(separator: "\n")
        encoded.append("\npayload_length=\(body.count)\n\n")
        var data = Data(encoded.utf8)
        data.append(body)
        return data
    }

    private static func splitHeaderAndBody(_ payload: Data) throws -> ([String: String], Data) {
        guard let separatorRange = payload.range(of: Data("\n\n".utf8)) else {
            throw BleRouteSyncPacketError.missingHeaderSeparator
        }
        let headerData = payload.subdata(in: 0..<separatorRange.lowerBound)
        let body = payload.subdata(in: separatorRange.upperBound..<payload.count)
        guard let headerText = String(data: headerData, encoding: .utf8) else {
            throw BleRouteSyncPacketError.invalidUtf8Header
        }
        let headers = Dictionary(uniqueKeysWithValues: headerText.split(separator: "\n").compactMap { line in
            guard let separator = line.firstIndex(of: "=") else { return nil }
            let key = String(line[..<separator]).trimmingCharacters(in: .whitespaces)
            let value = String(line[line.index(after: separator)...])
            return (key, value)
        })
        let expectedLength = try parseInt(try requiredField("payload_length", in: headers), field: "payload_length")
        guard expectedLength == body.count else {
            throw BleRouteSyncPacketError.payloadLengthMismatch(expected: expectedLength, actual: body.count)
        }
        return (headers, body)
    }

    private static func requiredField(_ key: String, in headers: [String: String]) throws -> String {
        guard let value = headers[key] else {
            throw BleRouteSyncPacketError.missingField(key)
        }
        return value
    }

    private static func optionalStringField(_ key: String, in headers: [String: String]) -> String? {
        guard let value = headers[key], !value.isEmpty, value != "none", value != "current" else {
            return nil
        }
        return value
    }

    private static func optionalIntField(_ key: String, in headers: [String: String]) throws -> Int? {
        guard let value = headers[key], !value.isEmpty, value != "none" else {
            return nil
        }
        return try parseInt(value, field: key)
    }

    private static func parseInt(_ value: String, field: String) throws -> Int {
        guard let intValue = Int(value) else {
            throw BleRouteSyncPacketError.invalidField(field: field, value: value)
        }
        return intValue
    }

    private static func parseDouble(_ value: String, field: String) throws -> Double {
        guard let doubleValue = Double(value) else {
            throw BleRouteSyncPacketError.invalidField(field: field, value: value)
        }
        return doubleValue
    }

    private static func nonEmpty(_ value: String) -> String? {
        value.isEmpty ? nil : value
    }
}

private extension Collection {
    subscript(safe index: Index) -> Element? {
        indices.contains(index) ? self[index] : nil
    }
}
