import Foundation
import Combine

@MainActor
final class BleRouteSyncService: ObservableObject, RouteSyncTransport {
    @Published private(set) var sessionState = SyncSessionState(
        connectionState: .disconnected,
        routeSyncState: .idle,
        lastSyncResult: "Not sent yet",
        lastDeviceName: nil,
        pendingRouteIdentifier: nil,
        pendingRouteRevision: nil,
        activeRouteIdentifier: nil,
        activeRouteRevision: nil,
        activeRouteChecksumHex: nil,
        transferProgress: nil,
        retryableInterruptionArmed: false,
        lastOutboundMessage: nil,
        lastInboundMessage: nil,
        lastStatusCode: nil
    )

    private let chunkSizeBytes = 96
    private var pendingTransfer: PendingTransfer?

    func scanForDevices() async {
        sessionState.connectionState = .scanning
        sessionState.lastDeviceName = "ESP32 Bike Minimap"
    }

    func connectToLastKnownDevice() async {
        sessionState.connectionState = .connecting
        sessionState.connectionState = .connected
        sessionState.lastDeviceName = sessionState.lastDeviceName ?? "ESP32 Bike Minimap"
    }

    func publishSet(_ route: NormalizedRoutePackage) async throws {
        try await beginTransfer(.set(RouteSetMessage(route: route)))
    }

    func publishUpdate(_ route: NormalizedRoutePackage) async throws {
        try await beginTransfer(
            .update(
                RouteUpdateMessage(
                    routeIdentifier: route.routeIdentifier,
                    revision: route.revision,
                    route: route
                )
            )
        )
    }

    func publishClear(routeIdentifier: String?) async throws {
        try await beginTransfer(.clear(RouteClearMessage(routeIdentifier: routeIdentifier)))
    }

    func resumePendingTransfer() async throws {
        guard let transfer = pendingTransfer else {
            throw TransferError.noPendingTransfer
        }
        sessionState.routeSyncState = .transferring
        sessionState.lastSyncResult = "Resuming \(transfer.message.kindLabel) at chunk \(transfer.nextChunkIndex + 1)/\(transfer.totalChunks)"
        try await drainPendingTransfer()
    }

    func armRetryableInterruptionOnNextTransfer() {
        sessionState.retryableInterruptionArmed = true
        sessionState.lastSyncResult = "Next transfer will simulate one retryable BLE interruption"
    }

    func receiveStatus(_ message: RouteStatusMessage) async {
        let decodedMessage = decodeInboundSyncMessage(.status(message))
        guard case .status(let message) = decodedMessage else { return }
        sessionState.lastInboundMessage = decodedMessage
        sessionState.lastStatusCode = message.status
        switch message.status {
        case .accepted, .applying:
            sessionState.routeSyncState = .awaitingAck
            sessionState.lastSyncResult = message.detail ?? "Waiting for device acknowledgement"
        case .active:
            sessionState.routeSyncState = .synced
            sessionState.activeRouteIdentifier = message.routeIdentifier
            sessionState.activeRouteRevision = message.revision
            sessionState.activeRouteChecksumHex = pendingTransfer?.checksumHex ?? sessionState.activeRouteChecksumHex
            sessionState.pendingRouteIdentifier = nil
            sessionState.pendingRouteRevision = nil
            sessionState.transferProgress = nil
            sessionState.lastSyncResult = message.detail ?? "Device activated route"
            pendingTransfer = nil
        case .cleared:
            sessionState.routeSyncState = .idle
            sessionState.pendingRouteIdentifier = nil
            sessionState.pendingRouteRevision = nil
            sessionState.activeRouteIdentifier = nil
            sessionState.activeRouteRevision = nil
            sessionState.activeRouteChecksumHex = nil
            sessionState.transferProgress = nil
            sessionState.lastSyncResult = message.detail ?? "Device cleared route"
            pendingTransfer = nil
        case .retryableFailure:
            sessionState.routeSyncState = .failed
            sessionState.lastSyncResult = message.detail ?? "Device reported retryable sync failure"
        case .rejected, .fatalFailure:
            sessionState.routeSyncState = .failed
            sessionState.pendingRouteIdentifier = nil
            sessionState.pendingRouteRevision = nil
            sessionState.transferProgress = nil
            sessionState.lastSyncResult = message.detail ?? "Device reported sync failure"
            pendingTransfer = nil
        }
    }

    func receiveRerouteRequest(_ message: RouteRerouteRequestMessage) async {
        let decodedMessage = decodeInboundSyncMessage(.rerouteRequest(message))
        guard case .rerouteRequest(let message) = decodedMessage else { return }
        sessionState.lastInboundMessage = decodedMessage
        sessionState.lastSyncResult = "Device requested reroute for \(message.routeIdentifier)"
    }

    private func beginTransfer(_ message: RouteSyncMessage) async throws {
        let payload = BleRouteSyncCodec.canonicalPayloadData(for: message)
        let totalChunks = max(1, Int(ceil(Double(payload.count) / Double(chunkSizeBytes))))
        let checksumHex = BleRouteSyncCodec.checksumHex(for: payload)
        let transfer = PendingTransfer(
            identifier: UUID().uuidString,
            message: message,
            payload: payload,
            checksumHex: checksumHex,
            totalChunks: totalChunks,
            nextChunkIndex: 0,
            retryCount: 0,
            lastError: nil
        )
        pendingTransfer = transfer
        sessionState.routeSyncState = .preparing
        sessionState.pendingRouteIdentifier = routeIdentifier(for: message)
        sessionState.pendingRouteRevision = routeRevision(for: message)
        sessionState.lastOutboundMessage = message
        sessionState.transferProgress = transfer.progress(chunkSizeBytes: chunkSizeBytes)
        sessionState.lastSyncResult = "Prepared \(message.kindLabel) payload (\(payload.count) B across \(totalChunks) chunks)"
        try await drainPendingTransfer()
    }

    private func drainPendingTransfer() async throws {
        guard var transfer = pendingTransfer else {
            throw TransferError.noPendingTransfer
        }

        sessionState.routeSyncState = .transferring
        while transfer.nextChunkIndex < transfer.totalChunks {
            try await Task.sleep(nanoseconds: 80_000_000)
            let chunkNumber = transfer.nextChunkIndex + 1

            if sessionState.retryableInterruptionArmed {
                sessionState.retryableInterruptionArmed = false
                transfer.retryCount += 1
                transfer.lastError = "Simulated BLE interruption at chunk \(chunkNumber)/\(transfer.totalChunks)"
                pendingTransfer = transfer
                sessionState.transferProgress = transfer.progress(chunkSizeBytes: chunkSizeBytes)
                await receiveStatus(
                    RouteStatusMessage(
                        routeIdentifier: transfer.routeIdentifier,
                        revision: transfer.routeRevision,
                        status: .retryableFailure,
                        detail: "Transfer interrupted at chunk \(chunkNumber)/\(transfer.totalChunks); tap Resume pending transfer"
                    )
                )
                return
            }

            transfer.nextChunkIndex = chunkNumber
            transfer.lastError = nil
            pendingTransfer = transfer
            sessionState.transferProgress = transfer.progress(chunkSizeBytes: chunkSizeBytes)
            sessionState.lastSyncResult = "Transferred chunk \(chunkNumber)/\(transfer.totalChunks) (\(transfer.progress(chunkSizeBytes: chunkSizeBytes).percentComplete)%)"
        }

        await receiveStatus(
            RouteStatusMessage(
                routeIdentifier: transfer.routeIdentifier,
                revision: transfer.routeRevision,
                status: .accepted,
                detail: "Checksum \(transfer.checksumHex) verified after \(transfer.totalChunks) chunks"
            )
        )
        await receiveStatus(
            RouteStatusMessage(
                routeIdentifier: transfer.routeIdentifier,
                revision: transfer.routeRevision,
                status: .applying,
                detail: "Applying route revision \(transfer.routeRevision.map(String.init) ?? "0") on device"
            )
        )
        let finalStatus = finalStatus(for: transfer)
        await receiveStatus(finalStatus)
    }

    private func finalStatus(for transfer: PendingTransfer) -> RouteStatusMessage {
        switch transfer.message {
        case .clear:
            return RouteStatusMessage(
                routeIdentifier: transfer.routeIdentifier,
                revision: nil,
                status: .cleared,
                detail: "Device cleared active route"
            )
        case .set(let message):
            return finalRouteStatus(for: message.route, checksumHex: transfer.checksumHex, kindLabel: transfer.message.kindLabel)
        case .update(let message):
            return finalRouteStatus(for: message.route, checksumHex: transfer.checksumHex, kindLabel: transfer.message.kindLabel)
        case .status, .rerouteRequest:
            return RouteStatusMessage(
                routeIdentifier: transfer.routeIdentifier,
                revision: transfer.routeRevision,
                status: .fatalFailure,
                detail: "Unsupported outbound sync message kind \(transfer.message.kindLabel)"
            )
        }
    }

    private func finalRouteStatus(for route: NormalizedRoutePackage, checksumHex: String, kindLabel: String) -> RouteStatusMessage {
        if let activeRouteIdentifier = sessionState.activeRouteIdentifier,
           activeRouteIdentifier == route.routeIdentifier,
           let activeRouteRevision = sessionState.activeRouteRevision {
            if route.revision < activeRouteRevision {
                return RouteStatusMessage(
                    routeIdentifier: route.routeIdentifier,
                    revision: route.revision,
                    status: .rejected,
                    detail: "Rejected stale route revision \(route.revision); device already has rev \(activeRouteRevision)"
                )
            }

            if route.revision == activeRouteRevision {
                if sessionState.activeRouteChecksumHex == checksumHex {
                    return RouteStatusMessage(
                        routeIdentifier: route.routeIdentifier,
                        revision: route.revision,
                        status: .active,
                        detail: "Duplicate \(kindLabel) replay deduped; existing route kept active"
                    )
                }
                return RouteStatusMessage(
                    routeIdentifier: route.routeIdentifier,
                    revision: route.revision,
                    status: .fatalFailure,
                    detail: "Revision conflict: route \(route.routeIdentifier) rev \(route.revision) has a different checksum"
                )
            }
        }

        return RouteStatusMessage(
            routeIdentifier: route.routeIdentifier,
            revision: route.revision,
            status: .active,
            detail: "Route revision \(route.revision) applied over BLE via \(kindLabel)"
        )
    }

    private func routeIdentifier(for message: RouteSyncMessage) -> String? {
        switch message {
        case .set(let message):
            return message.route.routeIdentifier
        case .update(let message):
            return message.routeIdentifier
        case .clear(let message):
            return message.routeIdentifier
        case .status(let message):
            return message.routeIdentifier
        case .rerouteRequest(let message):
            return message.routeIdentifier
        }
    }

    private func routeRevision(for message: RouteSyncMessage) -> Int? {
        switch message {
        case .set(let message):
            return message.route.revision
        case .update(let message):
            return message.revision
        case .clear:
            return nil
        case .status(let message):
            return message.revision
        case .rerouteRequest(let message):
            return message.revision
        }
    }

    private func canonicalPayloadData(for message: RouteSyncMessage) -> Data {
        Data(canonicalPayloadString(for: message).utf8)
    }

    private func canonicalPayloadString(for message: RouteSyncMessage) -> String {
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
                "revision=\(message.revision)",
                String(format: "rider=%.6f,%.6f", message.riderLocation.latitude, message.riderLocation.longitude),
                "reason=\(message.reason)"
            ].joined(separator: "\n")
        }
    }

    private func routePayloadString(kind: String, route: NormalizedRoutePackage) -> String {
        let geometry = route.geometry.map {
            String(format: "%.6f,%.6f", $0.latitude, $0.longitude)
        }.joined(separator: ";")
        let maneuvers = route.maneuvers.map { maneuver in
            [
                maneuver.id,
                maneuver.maneuverType.rawValue,
                String(format: "%.1f", maneuver.distanceFromStartMeters),
                String(format: "%.6f,%.6f", maneuver.location.latitude, maneuver.location.longitude),
                maneuver.instructionText ?? ""
            ].joined(separator: "|")
        }.joined(separator: ";")

        return [
            "kind=\(kind)",
            "route_id=\(route.routeIdentifier)",
            "revision=\(route.revision)",
            "version=\(route.version.major).\(route.version.minor)",
            String(format: "summary=%.1f|%d|%@|%@", route.summary.totalDistanceMeters, route.summary.estimatedDurationSeconds, route.summary.startLabel ?? "", route.summary.destinationLabel ?? ""),
            "geometry=\(geometry)",
            "maneuvers=\(maneuvers)",
            "provenance=\(route.provenance.providerID.rawValue)|\(route.provenance.sourceReference ?? "")|\(route.provenance.generatedAtUnixMs)"
        ].joined(separator: "\n")
    }

    private func checksumHex(for data: Data) -> String {
        var hash: UInt32 = 2_166_136_261
        for byte in data {
            hash ^= UInt32(byte)
            hash &*= 16_777_619
        }
        return String(format: "%08x", hash)
    }
}

private extension BleRouteSyncService {
    struct PendingTransfer {
        var identifier: String
        var message: RouteSyncMessage
        var payload: Data
        var checksumHex: String
        var totalChunks: Int
        var nextChunkIndex: Int
        var retryCount: Int
        var lastError: String?

        var routeIdentifier: String? {
            switch message {
            case .set(let message):
                return message.route.routeIdentifier
            case .update(let message):
                return message.routeIdentifier
            case .clear(let message):
                return message.routeIdentifier
            case .status(let message):
                return message.routeIdentifier
            case .rerouteRequest(let message):
                return message.routeIdentifier
            }
        }

        var routeRevision: Int? {
            switch message {
            case .set(let message):
                return message.route.revision
            case .update(let message):
                return message.revision
            case .clear:
                return nil
            case .status(let message):
                return message.revision
            case .rerouteRequest(let message):
                return message.revision
            }
        }

        func progress(chunkSizeBytes: Int) -> RouteTransferProgress {
            RouteTransferProgress(
                transferIdentifier: identifier,
                messageKind: message.kindLabel,
                routeIdentifier: routeIdentifier,
                routeRevision: routeRevision,
                payloadBytes: payload.count,
                chunkSizeBytes: chunkSizeBytes,
                totalChunks: totalChunks,
                acknowledgedChunks: nextChunkIndex,
                retryCount: retryCount,
                checksumHex: checksumHex,
                resumeChunkIndex: nextChunkIndex < totalChunks ? nextChunkIndex : nil,
                lastError: lastError
            )
        }
    }

    private func decodeInboundSyncMessage(_ message: RouteSyncMessage) -> RouteSyncMessage {
        do {
            let packet = BleRouteSyncPacket.syncMessage(message)
            let decodedPacket = try BleRouteSyncCodec.decode(BleRouteSyncCodec.encode(packet))
            if case .syncMessage(let decodedMessage) = decodedPacket {
                return decodedMessage
            }
        } catch {
            return message
        }
        return message
    }

    enum TransferError: Error {
        case noPendingTransfer
    }
}
