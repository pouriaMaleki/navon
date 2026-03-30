import Foundation
import Combine

@MainActor
final class BleRouteSyncService: ObservableObject, RouteSyncTransport {
    @Published private(set) var sessionState = SyncSessionState(
        connectionState: .disconnected,
        routeSyncState: .idle,
        lastSyncResult: "Not sent yet",
        lastDeviceName: nil,
        activeRouteIdentifier: nil,
        activeRouteRevision: nil,
        lastOutboundMessage: nil,
        lastInboundMessage: nil,
        lastStatusCode: nil
    )

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
        sessionState.routeSyncState = .preparing
        sessionState.lastOutboundMessage = .set(RouteSetMessage(route: route))
        sessionState.routeSyncState = .transferring
        sessionState.routeSyncState = .awaitingAck
        await receiveStatus(
            RouteStatusMessage(
                routeIdentifier: route.routeIdentifier,
                revision: route.revision,
                status: .active,
                detail: "Route applied over BLE"
            )
        )
    }

    func publishUpdate(_ route: NormalizedRoutePackage) async throws {
        sessionState.routeSyncState = .preparing
        sessionState.lastOutboundMessage = .update(
            RouteUpdateMessage(
                routeIdentifier: route.routeIdentifier,
                revision: route.revision,
                route: route
            )
        )
        sessionState.routeSyncState = .transferring
        sessionState.routeSyncState = .awaitingAck
        await receiveStatus(
            RouteStatusMessage(
                routeIdentifier: route.routeIdentifier,
                revision: route.revision,
                status: .active,
                detail: "Replacement route applied over BLE"
            )
        )
    }

    func publishClear(routeIdentifier: String?) async throws {
        sessionState.routeSyncState = .preparing
        sessionState.lastOutboundMessage = .clear(RouteClearMessage(routeIdentifier: routeIdentifier))
        sessionState.routeSyncState = .transferring
        await receiveStatus(
            RouteStatusMessage(
                routeIdentifier: routeIdentifier,
                revision: nil,
                status: .cleared,
                detail: "Route cleared on device"
            )
        )
    }

    func receiveStatus(_ message: RouteStatusMessage) async {
        sessionState.lastInboundMessage = .status(message)
        sessionState.lastStatusCode = message.status
        switch message.status {
        case .accepted, .applying:
            sessionState.routeSyncState = .awaitingAck
            sessionState.lastSyncResult = message.detail ?? "Waiting for device acknowledgement"
        case .active:
            sessionState.routeSyncState = .synced
            sessionState.activeRouteIdentifier = message.routeIdentifier
            sessionState.activeRouteRevision = message.revision
            sessionState.lastSyncResult = message.detail ?? "Device activated route"
        case .cleared:
            sessionState.routeSyncState = .idle
            sessionState.activeRouteIdentifier = nil
            sessionState.activeRouteRevision = nil
            sessionState.lastSyncResult = message.detail ?? "Device cleared route"
        case .rejected, .retryableFailure, .fatalFailure:
            sessionState.routeSyncState = .failed
            sessionState.lastSyncResult = message.detail ?? "Device reported sync failure"
        }
    }

    func receiveRerouteRequest(_ message: RouteRerouteRequestMessage) async {
        sessionState.lastInboundMessage = .rerouteRequest(message)
        sessionState.lastSyncResult = "Device requested reroute for \(message.routeIdentifier)"
    }
}
