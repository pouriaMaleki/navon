import Foundation
import Combine

@MainActor
final class BleRouteSyncService: ObservableObject, RouteSyncTransport {
    @Published private(set) var sessionState = SyncSessionState(
        connectionState: .disconnected,
        routeSyncState: .idle,
        lastSyncResult: "Not sent yet",
        lastDeviceName: nil
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

    func sendRoute(_ route: NormalizedRoutePackage) async throws {
        sessionState.routeSyncState = .preparing
        sessionState.routeSyncState = .transferring
        sessionState.routeSyncState = .awaitingAck
        sessionState.routeSyncState = .synced
        sessionState.lastSyncResult = "Synced route \(route.routeIdentifier) rev \(route.revision) • \(route.geometryPointCount) pts / \(route.maneuverCount) maneuvers"
    }

    func clearRoute(routeIdentifier: String?) async throws {
        sessionState.routeSyncState = .idle
        sessionState.lastSyncResult = "Cleared route \(routeIdentifier ?? "current")"
    }
}
