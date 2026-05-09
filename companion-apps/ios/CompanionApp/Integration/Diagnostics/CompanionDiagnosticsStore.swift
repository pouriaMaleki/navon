import Foundation
import Combine

final class CompanionDiagnosticsStore: ObservableObject {
    @Published var diagnostics = CompanionDiagnostics(
        providerName: "HSL",
        routeIdentifier: "none",
        routeRevision: 0,
        bleState: DeviceConnectionState.disconnected.rawValue,
        lastSyncResult: "Not sent yet",
        lastRerouteOutcome: "No reroute yet"
    )

    func update(from session: ActiveRouteSession?, syncState: SyncSessionState) {
        diagnostics = CompanionDiagnostics(
            providerName: session?.providerID.displayName ?? "HSL",
            routeIdentifier: session?.routeIdentifier ?? "none",
            routeRevision: session?.routeRevision ?? 0,
            bleState: syncState.connectionState.rawValue,
            lastSyncResult: syncState.lastSyncResult,
            lastRerouteOutcome: session?.lastRerouteReason ?? "No reroute yet"
        )
    }
}
