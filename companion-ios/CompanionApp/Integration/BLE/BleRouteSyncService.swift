import Foundation
import Combine

@MainActor
final class BleRouteSyncService: ObservableObject, RouteSyncTransport {
    @Published var sessionState = SyncSessionState(
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
        armedFaultInjectionMode: nil,
        lastOutboundMessage: nil,
        lastInboundMessage: nil,
        lastStatusCode: nil
    )

    private let chunkSizeBytes = 96
    private let ackTimeoutNanoseconds: UInt64 = 2_000_000_000
    private let bluetoothClient: RouteSyncBluetoothClient
    private var pendingTransfer: PendingTransfer?
    private var ackTimeoutTask: Task<Void, Never>?

    init(bluetoothClient: RouteSyncBluetoothClient = CoreBluetoothRouteSyncClient()) {
        self.bluetoothClient = bluetoothClient
        self.bluetoothClient.onConnectionStateChange = { [weak self] state, name in
            Task { @MainActor in
                self?.handleConnectionStateChange(state, name: name)
            }
        }
        self.bluetoothClient.onSyncMessage = { [weak self] message in
            Task { @MainActor in
                await self?.handleInbound(message)
            }
        }
    }

    func scanForDevices() async {
        sessionState.connectionState = .scanning
        do {
            sessionState.lastDeviceName = try await bluetoothClient.scanForRouteSyncPeripheral(timeout: 6.0)
            sessionState.lastSyncResult = "Discovered \(sessionState.lastDeviceName ?? "ESP32 Bike Minimap")"
        } catch {
            sessionState.connectionState = .disconnected
            sessionState.lastSyncResult = error.localizedDescription
        }
    }

    func connectToLastKnownDevice() async {
        sessionState.connectionState = .connecting
        do {
            sessionState.lastDeviceName = try await bluetoothClient.connectToScannedPeripheral()
            sessionState.connectionState = .connected
            sessionState.lastSyncResult = "Connected to \(sessionState.lastDeviceName ?? "ESP32 Bike Minimap")"
        } catch {
            sessionState.connectionState = .disconnected
            sessionState.lastSyncResult = error.localizedDescription
        }
    }

    /// Fast-path reconnect to an already-paired peripheral using its persisted
    /// CoreBluetooth identifier. Skips the scan entirely. On `noDiscoveredPeripheral`
    /// (iOS bond store empty after a fresh install) the caller is expected to
    /// fall back to `scanForDevices` + `connectToLastKnownDevice`.
    func connectToPairedPeripheral(identifier: String) async {
        sessionState.connectionState = .connecting
        do {
            sessionState.lastDeviceName = try await bluetoothClient.connectToPairedPeripheral(identifier: identifier)
            sessionState.connectionState = .connected
            sessionState.lastSyncResult = "Connected to \(sessionState.lastDeviceName ?? "ESP32 Bike Minimap")"
        } catch {
            sessionState.connectionState = .disconnected
            sessionState.lastSyncResult = error.localizedDescription
        }
    }

    /// Pairing-flow connect: scan for the route-sync service UUID and connect
    /// to whichever peripheral advertises it. iOS does not get an identifier
    /// from the QR — the returned `ConnectedPeripheralInfo.identifier` is
    /// captured at connect time and persisted by the caller for future
    /// fast-path reconnects. Throws on failure so the caller
    /// (`AppModel.completePairing`) can surface a UI error without persisting
    /// a half-state.
    func connectToAdvertisedPeripheral() async throws -> ConnectedPeripheralInfo {
        sessionState.connectionState = .connecting
        do {
            let info = try await bluetoothClient.connectToAdvertisedPeripheral()
            sessionState.lastDeviceName = info.name
            sessionState.connectionState = .connected
            sessionState.lastSyncResult = "Connected to \(info.name)"
            return info
        } catch {
            sessionState.connectionState = .disconnected
            sessionState.lastSyncResult = error.localizedDescription
            throw error
        }
    }

    /// Forwards the OOB confirmation write to the BLE client. Throws on
    /// failure so the AppModel can avoid persisting the bond when the
    /// device rejects the secret.
    func writePairingConfirm(secret: Data) async throws {
        try await bluetoothClient.writePairingConfirm(secret: secret)
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
        guard var transfer = pendingTransfer else {
            throw TransferError.noPendingTransfer
        }
        if transfer.nextChunkIndex >= transfer.chunkEnvelopes.count {
            transfer.nextChunkIndex = 0
            pendingTransfer = transfer
            sessionState.transferProgress = transfer.progress(chunkSizeBytes: chunkSizeBytes)
        }
        sessionState.routeSyncState = .transferring
        try await drainPendingTransfer()
    }

    func armRetryableInterruptionOnNextTransfer() {
        armFaultInjection(.retryableInterruption)
    }

    func armFaultInjection(_ mode: RouteSyncFaultInjectionMode) {
        sessionState.armedFaultInjectionMode = mode
        switch mode {
        case .retryableInterruption:
            sessionState.retryableInterruptionArmed = true
        case .writeFailure, .disconnectAfterChunkWrite, .dropNextInboundStatus:
            sessionState.retryableInterruptionArmed = false
            bluetoothClient.armDebugFault(mode)
        }
        sessionState.lastSyncResult = "Armed \(mode.displayName.lowercased()) for the next BLE sync cycle"
    }

    func receiveStatus(_ message: RouteStatusMessage) async {
        await handleInbound(.status(message))
    }

    func receiveRerouteRequest(_ message: RouteRerouteRequestMessage) async {
        await handleInbound(.rerouteRequest(message))
    }

    private func beginTransfer(_ message: RouteSyncMessage) async throws {
        cancelAckTimeout()
        let transferIdentifier = UUID().uuidString
        let payload = BleRouteSyncCodec.canonicalPayloadData(for: message)
        let transfer = PendingTransfer(
            identifier: transferIdentifier,
            message: message,
            payload: payload,
            checksumHex: BleRouteSyncCodec.checksumHex(for: payload),
            chunkEnvelopes: BleRouteSyncCodec.chunkEnvelopes(
                for: message,
                transferIdentifier: transferIdentifier,
                chunkSizeBytes: chunkSizeBytes
            ),
            nextChunkIndex: 0,
            retryCount: 0,
            lastError: nil,
            usingLiveTransport: bluetoothClient.isReady
        )
        pendingTransfer = transfer
        sessionState.routeSyncState = .preparing
        sessionState.pendingRouteIdentifier = transfer.routeIdentifier
        sessionState.pendingRouteRevision = transfer.routeRevision
        sessionState.lastOutboundMessage = message
        sessionState.transferProgress = transfer.progress(chunkSizeBytes: chunkSizeBytes)
        sessionState.lastSyncResult = "Prepared \(message.kindLabel) payload (\(transfer.payload.count) B across \(transfer.chunkEnvelopes.count) chunks)"
        try await drainPendingTransfer()
    }

    private func drainPendingTransfer() async throws {
        guard var transfer = pendingTransfer else {
            throw TransferError.noPendingTransfer
        }

        cancelAckTimeout()
        sessionState.routeSyncState = .transferring
        while transfer.nextChunkIndex < transfer.chunkEnvelopes.count {
            try await Task.sleep(nanoseconds: 80_000_000)
            let chunkNumber = transfer.nextChunkIndex + 1

            if sessionState.retryableInterruptionArmed {
                sessionState.retryableInterruptionArmed = false
                sessionState.armedFaultInjectionMode = nil
                transfer.retryCount += 1
                transfer.lastError = "Simulated BLE interruption at chunk \(chunkNumber)/\(transfer.chunkEnvelopes.count)"
                pendingTransfer = transfer
                sessionState.transferProgress = transfer.progress(chunkSizeBytes: chunkSizeBytes)
                sessionState.routeSyncState = .failed
                sessionState.lastSyncResult = transfer.lastError ?? "Transfer interrupted"
                return
            }

            if transfer.usingLiveTransport && !bluetoothClient.isReady {
                failTransferDueToDisconnect(&transfer, detail: "BLE transport disconnected before chunk \(chunkNumber)/\(transfer.chunkEnvelopes.count) could be written")
                return
            }

            let envelope = transfer.chunkEnvelopes[transfer.nextChunkIndex]
            if transfer.usingLiveTransport {
                do {
                    try await bluetoothClient.write(packet: .chunk(envelope))
                } catch {
                    failTransfer(&transfer, detail: error.localizedDescription, restartFromFirstChunk: false)
                    return
                }
            }

            transfer.nextChunkIndex = chunkNumber
            transfer.lastError = nil
            pendingTransfer = transfer
            sessionState.transferProgress = transfer.progress(chunkSizeBytes: chunkSizeBytes)
            sessionState.lastSyncResult = "Transferred chunk \(chunkNumber)/\(transfer.chunkEnvelopes.count) (\(transfer.progress(chunkSizeBytes: chunkSizeBytes).percentComplete)%)"
        }

        if transfer.usingLiveTransport {
            guard bluetoothClient.isReady else {
                failTransferDueToDisconnect(&transfer, detail: "BLE transport disconnected after chunk upload finished; full transfer must be replayed")
                return
            }
            sessionState.routeSyncState = .awaitingAck
            sessionState.lastSyncResult = "Waiting for ESP32 acknowledgement over BLE"
            scheduleAckTimeout()
        } else {
            await simulateDeviceCompletion(for: transfer)
        }
    }

    private func simulateDeviceCompletion(for transfer: PendingTransfer) async {
        await receiveStatus(
            RouteStatusMessage(
                routeIdentifier: transfer.routeIdentifier,
                revision: transfer.routeRevision,
                status: .accepted,
                detail: "Checksum \(transfer.checksumHex) verified after \(transfer.chunkEnvelopes.count) chunks"
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
        await receiveStatus(finalStatus(for: transfer))
    }

    private func handleInbound(_ message: RouteSyncMessage) async {
        let decodedMessage = decodeInboundSyncMessage(message)
        sessionState.lastInboundMessage = decodedMessage

        switch decodedMessage {
        case .status(let status):
            sessionState.lastStatusCode = status.status
            switch status.status {
            case .accepted, .applying:
                sessionState.routeSyncState = .awaitingAck
                sessionState.lastSyncResult = status.detail ?? "Waiting for device acknowledgement"
                scheduleAckTimeout()
            case .active:
                cancelAckTimeout()
                sessionState.routeSyncState = .synced
                sessionState.activeRouteIdentifier = status.routeIdentifier
                sessionState.activeRouteRevision = status.revision
                sessionState.activeRouteChecksumHex = pendingTransfer?.checksumHex ?? sessionState.activeRouteChecksumHex
                sessionState.pendingRouteIdentifier = nil
                sessionState.pendingRouteRevision = nil
                sessionState.transferProgress = nil
                sessionState.lastSyncResult = status.detail ?? "Device activated route"
                sessionState.armedFaultInjectionMode = nil
                pendingTransfer = nil
            case .cleared:
                cancelAckTimeout()
                sessionState.routeSyncState = .idle
                sessionState.pendingRouteIdentifier = nil
                sessionState.pendingRouteRevision = nil
                sessionState.activeRouteIdentifier = nil
                sessionState.activeRouteRevision = nil
                sessionState.activeRouteChecksumHex = nil
                sessionState.transferProgress = nil
                sessionState.lastSyncResult = status.detail ?? "Device cleared route"
                sessionState.armedFaultInjectionMode = nil
                pendingTransfer = nil
            case .retryableFailure:
                cancelAckTimeout()
                if var transfer = pendingTransfer {
                    transfer.retryCount += 1
                    transfer.lastError = status.detail ?? "Device reported retryable sync failure"
                    transfer.nextChunkIndex = 0
                    pendingTransfer = transfer
                    sessionState.transferProgress = transfer.progress(chunkSizeBytes: chunkSizeBytes)
                }
                sessionState.routeSyncState = .failed
                sessionState.lastSyncResult = status.detail ?? "Device reported retryable sync failure"
                sessionState.armedFaultInjectionMode = nil
            case .rejected, .fatalFailure:
                cancelAckTimeout()
                sessionState.routeSyncState = .failed
                sessionState.pendingRouteIdentifier = nil
                sessionState.pendingRouteRevision = nil
                sessionState.transferProgress = nil
                sessionState.lastSyncResult = status.detail ?? "Device reported sync failure"
                sessionState.armedFaultInjectionMode = nil
                pendingTransfer = nil
            }
        case .rerouteRequest(let request):
            sessionState.lastSyncResult = "Device requested reroute for \(request.routeIdentifier)"
        case .set, .update, .clear:
            sessionState.lastSyncResult = "Received unexpected inbound \(decodedMessage.kindLabel) message"
        }
    }

    private func handleConnectionStateChange(_ state: DeviceConnectionState, name: String?) {
        sessionState.connectionState = state
        if let name {
            sessionState.lastDeviceName = name
        }
        guard state == .disconnected else {
            return
        }

        cancelAckTimeout()
        if var transfer = pendingTransfer, transfer.usingLiveTransport {
            failTransferDueToDisconnect(&transfer, detail: "BLE transport disconnected; reconnect and resume the pending transfer")
        } else {
            sessionState.routeSyncState = .idle
        }
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
            return finalRouteStatus(for: message.route, checksumHex: transfer.checksumHex, kindLabel: transfer.message.kindLabel, usingLiveTransport: transfer.usingLiveTransport)
        case .update(let message):
            return finalRouteStatus(for: message.route, checksumHex: transfer.checksumHex, kindLabel: transfer.message.kindLabel, usingLiveTransport: transfer.usingLiveTransport)
        case .status, .rerouteRequest:
            return RouteStatusMessage(
                routeIdentifier: transfer.routeIdentifier,
                revision: transfer.routeRevision,
                status: .fatalFailure,
                detail: "Unsupported outbound sync message kind \(transfer.message.kindLabel)"
            )
        }
    }

    private func finalRouteStatus(for route: NormalizedRoutePackage, checksumHex: String, kindLabel: String, usingLiveTransport: Bool) -> RouteStatusMessage {
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
            detail: usingLiveTransport
                ? "Route revision \(route.revision) applied on ESP32 over CoreBluetooth via \(kindLabel)"
                : "Route revision \(route.revision) applied over simulated BLE via \(kindLabel)"
        )
    }

    private func decodeInboundSyncMessage(_ message: RouteSyncMessage) -> RouteSyncMessage {
        do {
            let packet = BleRouteSyncPacket.syncMessage(message)
            let decodedPacket = try BleRouteSyncCodec.decode(BleRouteSyncCodec.encode(packet))
            if case let .syncMessage(decodedMessage) = decodedPacket {
                return decodedMessage
            }
        } catch {
            return message
        }
        return message
    }

    private func failTransfer(_ transfer: inout PendingTransfer, detail: String, restartFromFirstChunk: Bool) {
        cancelAckTimeout()
        transfer.retryCount += 1
        transfer.lastError = detail
        if restartFromFirstChunk || transfer.nextChunkIndex >= transfer.chunkEnvelopes.count {
            transfer.nextChunkIndex = 0
        }
        pendingTransfer = transfer
        sessionState.transferProgress = transfer.progress(chunkSizeBytes: chunkSizeBytes)
        sessionState.routeSyncState = .failed
        sessionState.lastSyncResult = detail
        sessionState.armedFaultInjectionMode = nil
    }

    private func failTransferDueToDisconnect(_ transfer: inout PendingTransfer, detail: String) {
        failTransfer(&transfer, detail: detail, restartFromFirstChunk: transfer.nextChunkIndex >= transfer.chunkEnvelopes.count)
    }

    private func scheduleAckTimeout() {
        cancelAckTimeout()
        ackTimeoutTask = Task { [weak self] in
            guard let self else { return }
            try? await Task.sleep(nanoseconds: self.ackTimeoutNanoseconds)
            await MainActor.run {
                guard self.sessionState.routeSyncState == .awaitingAck,
                      var transfer = self.pendingTransfer else { return }
                transfer.retryCount += 1
                transfer.lastError = "Timed out waiting for ESP32 acknowledgement; replay the route transfer"
                transfer.nextChunkIndex = 0
                self.pendingTransfer = transfer
                self.sessionState.transferProgress = transfer.progress(chunkSizeBytes: self.chunkSizeBytes)
                self.sessionState.routeSyncState = .failed
                self.sessionState.lastSyncResult = transfer.lastError ?? "Timed out waiting for device acknowledgement"
                self.sessionState.armedFaultInjectionMode = nil
            }
        }
    }

    private func cancelAckTimeout() {
        ackTimeoutTask?.cancel()
        ackTimeoutTask = nil
    }
}

private extension BleRouteSyncService {
    struct PendingTransfer {
        var identifier: String
        var message: RouteSyncMessage
        var payload: Data
        var checksumHex: String
        var chunkEnvelopes: [RouteTransferChunkEnvelope]
        var nextChunkIndex: Int
        var retryCount: Int
        var lastError: String?
        var usingLiveTransport: Bool

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
            case .clear, .rerouteRequest:
                return nil
            case .status(let message):
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
                totalChunks: chunkEnvelopes.count,
                acknowledgedChunks: nextChunkIndex,
                retryCount: retryCount,
                checksumHex: checksumHex,
                resumeChunkIndex: nextChunkIndex < chunkEnvelopes.count ? nextChunkIndex : nil,
                lastError: lastError
            )
        }
    }

    enum TransferError: LocalizedError {
        case noPendingTransfer

        var errorDescription: String? {
            switch self {
            case .noPendingTransfer:
                return "There is no pending BLE route transfer to resume"
            }
        }
    }
}
