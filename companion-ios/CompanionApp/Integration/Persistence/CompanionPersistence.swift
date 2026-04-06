import Foundation

final class CompanionPersistence: RouteSessionStore {
    private var destinations: [CoordinatePoint] = []
    private var routeHistory: [RouteHistoryItem] = []
    private var lastSession: ActiveRouteSession?
    private var settings = CompanionSettings.defaults
    private var plannerPreferences = RoutePlannerPreferences.defaults

    func loadRecentDestinations() -> [CoordinatePoint] {
        destinations
    }

    func saveRecentDestination(_ point: CoordinatePoint) {
        destinations.removeAll { $0 == point }
        destinations.insert(point, at: 0)
        destinations = Array(destinations.prefix(30))
    }

    func loadRecentRouteHistory() -> [RouteHistoryItem] {
        routeHistory
    }

    func saveRouteHistoryItem(_ item: RouteHistoryItem) {
        routeHistory.removeAll { $0.id == item.id }
        routeHistory.insert(item, at: 0)
        routeHistory = Array(routeHistory.prefix(50))
    }

    func dismissRouteHistoryItem(id: String) {
        routeHistory.removeAll { $0.id == id }
    }

    func loadLastSession() -> ActiveRouteSession? {
        lastSession
    }

    func saveSession(_ session: ActiveRouteSession) {
        lastSession = session
    }

    func loadSettings() -> CompanionSettings {
        settings
    }

    func saveSettings(_ newSettings: CompanionSettings) {
        settings = newSettings
    }

    func loadRoutePlannerPreferences() -> RoutePlannerPreferences {
        plannerPreferences
    }

    func saveRoutePlannerPreferences(_ preferences: RoutePlannerPreferences) {
        plannerPreferences = preferences
    }
}
