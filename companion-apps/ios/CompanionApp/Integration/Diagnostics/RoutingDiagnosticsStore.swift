import Foundation

@MainActor
final class RoutingDiagnosticsStore: ObservableObject {
    @Published var sessions: [RoutingDiagSession] = []
    @Published var currentSession: RoutingDiagSession? = nil

    private let persistence: CompanionPersistence
    private var eventCounter = 0

    var isRecording: Bool { currentSession != nil }

    init(persistence: CompanionPersistence) {
        self.persistence = persistence
        self.sessions = persistence.loadRoutingDiagnosticsSessions()
    }

    func startRecording() {
        guard currentSession == nil else { return }
        currentSession = RoutingDiagSession(
            id: newSessionId(),
            createdAtMs: nowMs(),
            updatedAtMs: nowMs(),
            events: []
        )
    }

    func stopRecording() {
        guard let session = currentSession else { return }
        var finalized = session
        finalized.updatedAtMs = nowMs()
        currentSession = nil
        persistence.saveRoutingDiagnosticsSession(finalized)
        sessions = persistence.loadRoutingDiagnosticsSessions()
    }

    func recordEvent(_ data: RoutingDiagEventData) {
        guard currentSession != nil else { return }
        let event = RoutingDiagEvent(
            id: newEventId(&eventCounter),
            timestampMs: nowMs(),
            data: data
        )
        currentSession?.events.append(event)
        currentSession?.updatedAtMs = nowMs()
    }

    func deleteSession(id: String) {
        persistence.dismissRoutingDiagnosticsSession(id: id)
        sessions = persistence.loadRoutingDiagnosticsSessions()
    }

    func debugPackageText(for id: String) -> String? {
        sessions.first(where: { $0.id == id })?.debugPackageText
    }

    private func nowMs() -> UInt64 {
        UInt64(Date().timeIntervalSince1970 * 1000)
    }
}
