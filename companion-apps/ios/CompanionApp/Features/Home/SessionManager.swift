import Foundation

/// Wraps `ActiveRouteSession` with auto-persistence. Every mutation to `session`
/// after init is automatically persisted, removing the need for explicit
/// `persistence.saveSession(...)` calls scattered throughout AppModel.
@MainActor
final class SessionManager: ObservableObject {
    @Published var session: ActiveRouteSession {
        didSet {
            persistence.saveSession(session)
        }
    }

    private let persistence: CompanionPersistence

    init(persistence: CompanionPersistence) {
        self.persistence = persistence
        if let stored = persistence.loadLastSession() {
            var cleaned = stored
            cleaned.routeIdentifier = nil
            cleaned.routeRevision = nil
            self.session = cleaned
            persistence.saveSession(cleaned)
        } else {
            self.session = ActiveRouteSession(
                routeIdentifier: nil,
                routeRevision: nil,
                destinationLabel: "No destination",
                destinationCoordinate: nil,
                providerID: .hsl,
                sourceMode: .mixed,
                lastRerouteReason: nil,
                lastRerouteTimestamp: nil
            )
        }
    }

    func save() {
        persistence.saveSession(session)
    }
}
