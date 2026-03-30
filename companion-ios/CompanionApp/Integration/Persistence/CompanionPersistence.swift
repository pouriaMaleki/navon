import Foundation

final class CompanionPersistence: RouteSessionStore {
    private var destinations: [CoordinatePoint] = []
    private var lastSession: ActiveRouteSession?

    func loadRecentDestinations() -> [CoordinatePoint] {
        destinations
    }

    func saveRecentDestination(_ point: CoordinatePoint) {
        destinations.insert(point, at: 0)
        destinations = Array(destinations.prefix(10))
    }

    func loadLastSession() -> ActiveRouteSession? {
        lastSession
    }

    func saveSession(_ session: ActiveRouteSession) {
        lastSession = session
    }
}
