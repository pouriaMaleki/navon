import Foundation

extension AppModel {
    var routingDiagnosticsSessions: [RoutingDiagSession] {
        routingDiagnosticsStore.sessions
    }

    func dismissRoutingDiagnosticsSession(id: String) {
        routingDiagnosticsStore.deleteSession(id: id)
        notePersistenceChanged()
    }

    /// Called by the settings toggle and lifecycle handlers.
    func applyRoutingDiagnosticsRecording(enabled: Bool) {
        if enabled {
            routingDiagnosticsStore.startRecording()
        } else {
            routingDiagnosticsStore.stopRecording()
        }
    }
}
