import Foundation

final class CompanionPersistence: RouteSessionStore {
    private enum Key {
        static let destinations = "companion.recentDestinations"
        static let routeHistory = "companion.routeHistory"
        static let lastSession = "companion.lastSession"
        static let settings = "companion.settings"
        static let plannerPreferences = "companion.routePlannerPreferences"
    }

    private let defaults: UserDefaults
    private let encoder = JSONEncoder()
    private let decoder = JSONDecoder()

    init(defaults: UserDefaults = .standard) {
        self.defaults = defaults
    }

    func loadRecentDestinations() -> [CoordinatePoint] {
        load([CoordinatePoint].self, forKey: Key.destinations) ?? []
    }

    func saveRecentDestination(_ point: CoordinatePoint) {
        var destinations = loadRecentDestinations()
        destinations.removeAll { $0 == point }
        destinations.insert(point, at: 0)
        save(Array(destinations.prefix(30)), forKey: Key.destinations)
    }

    func loadRecentRouteHistory() -> [RouteHistoryItem] {
        load([RouteHistoryItem].self, forKey: Key.routeHistory) ?? []
    }

    func saveRouteHistoryItem(_ item: RouteHistoryItem) {
        var routeHistory = loadRecentRouteHistory()
        routeHistory.removeAll { $0.id == item.id }
        routeHistory.insert(item, at: 0)
        save(Array(routeHistory.prefix(50)), forKey: Key.routeHistory)
    }

    func dismissRouteHistoryItem(id: String) {
        var routeHistory = loadRecentRouteHistory()
        routeHistory.removeAll { $0.id == id }
        save(routeHistory, forKey: Key.routeHistory)
    }

    func loadLastSession() -> ActiveRouteSession? {
        load(ActiveRouteSession.self, forKey: Key.lastSession)
    }

    func saveSession(_ session: ActiveRouteSession) {
        save(session, forKey: Key.lastSession)
    }

    func loadSettings() -> CompanionSettings {
        load(CompanionSettings.self, forKey: Key.settings) ?? .defaults
    }

    func saveSettings(_ newSettings: CompanionSettings) {
        save(newSettings, forKey: Key.settings)
    }

    func loadRoutePlannerPreferences() -> RoutePlannerPreferences {
        load(RoutePlannerPreferences.self, forKey: Key.plannerPreferences) ?? .defaults
    }

    func saveRoutePlannerPreferences(_ preferences: RoutePlannerPreferences) {
        save(preferences, forKey: Key.plannerPreferences)
    }

    private func load<T: Decodable>(_ type: T.Type, forKey key: String) -> T? {
        guard let data = defaults.data(forKey: key) else { return nil }
        return try? decoder.decode(type, from: data)
    }

    private func save<T: Encodable>(_ value: T, forKey key: String) {
        guard let data = try? encoder.encode(value) else { return }
        defaults.set(data, forKey: key)
    }
}
