import Foundation

final class CompanionPersistence: RouteSessionStore {
    private enum Key {
        static let destinations = "companion.recentDestinations"
        static let routeHistory = "companion.routeHistory"
        static let importDiagnostics = "companion.importDiagnostics"
        static let pendingHomeImportPresentation = "companion.pendingHomeImportPresentation"
        static let lastSession = "companion.lastSession"
        static let settings = "companion.settings"
        static let plannerPreferences = "companion.routePlannerPreferences"
        static let lastKnownRider = "companion.lastKnownRider"
        static let pairedPeripheral = "companion.pairedPeripheral"
        static let routingDiagnostics = "companion.routingDiagnostics"
    }

    private let defaults: UserDefaults
    private let encoder = JSONEncoder()
    private let decoder = JSONDecoder()
    private let nearbyDestinationMergeThresholdMeters = 80.0

    /// Encoder used for `PairedPeripheralRecord` so its `pairedAt` round-trips
    /// the cross-platform parity fixture (ISO-8601 with milliseconds + `Z`).
    /// The default `JSONEncoder` strategies don't emit fractional seconds,
    /// which would diverge from Android's record bytes.
    private let pairedPeripheralEncoder: JSONEncoder = {
        let encoder = JSONEncoder()
        encoder.dateEncodingStrategy = .formatted(CompanionPersistence.iso8601MillisFormatter)
        return encoder
    }()
    private let pairedPeripheralDecoder: JSONDecoder = {
        let decoder = JSONDecoder()
        decoder.dateDecodingStrategy = .formatted(CompanionPersistence.iso8601MillisFormatter)
        return decoder
    }()

    static let iso8601MillisFormatter: DateFormatter = {
        let formatter = DateFormatter()
        formatter.calendar = Calendar(identifier: .iso8601)
        formatter.locale = Locale(identifier: "en_US_POSIX")
        formatter.timeZone = TimeZone(secondsFromGMT: 0)
        formatter.dateFormat = "yyyy-MM-dd'T'HH:mm:ss.SSS'Z'"
        return formatter
    }()

    init(defaults: UserDefaults = .standard) {
        self.defaults = defaults
    }

    func loadRecentDestinations() -> [CoordinatePoint] {
        load([CoordinatePoint].self, forKey: Key.destinations) ?? []
    }

    func saveRecentDestination(_ point: CoordinatePoint) {
        var destinations = loadRecentDestinations()
        destinations.removeAll { areNearby($0, point) }
        destinations.insert(point, at: 0)
        save(Array(destinations.prefix(30)), forKey: Key.destinations)
    }

    func loadRecentRouteHistory() -> [RouteHistoryItem] {
        load([RouteHistoryItem].self, forKey: Key.routeHistory) ?? []
    }

    func saveRouteHistoryItem(_ item: RouteHistoryItem) {
        var routeHistory = loadRecentRouteHistory()

        if item.source == .recentDestination,
           let destination = item.destination,
           let existingIndex = routeHistory.firstIndex(where: { candidate in
               candidate.source == .recentDestination && candidate.destination.map { areNearby($0, destination) } == true
           }) {
            let existing = routeHistory.remove(at: existingIndex)
            let merged = RouteHistoryItem(
                id: existing.id,
                title: preferredDestinationTitle(newTitle: item.title, existingTitle: existing.title),
                subtitle: item.subtitle,
                source: .recentDestination,
                sourceLabel: item.sourceLabel,
                createdAt: item.createdAt,
                destination: item.destination ?? existing.destination,
                routePackage: nil,
                occurrenceCount: (existing.occurrenceCount ?? 1) + max(item.occurrenceCount ?? 1, 1)
            )
            routeHistory.insert(merged, at: 0)
            save(Array(routeHistory.prefix(50)), forKey: Key.routeHistory)
            return
        }

        routeHistory.removeAll { $0.id == item.id }
        routeHistory.insert(item, at: 0)
        save(Array(routeHistory.prefix(50)), forKey: Key.routeHistory)
    }

    func dismissRouteHistoryItem(id: String) {
        var routeHistory = loadRecentRouteHistory()
        routeHistory.removeAll { $0.id == id }
        save(routeHistory, forKey: Key.routeHistory)
    }

    func loadImportDiagnostics() -> [ImportDiagnosticsEntry] {
        load([ImportDiagnosticsEntry].self, forKey: Key.importDiagnostics) ?? []
    }

    func saveImportDiagnosticsEntry(_ entry: ImportDiagnosticsEntry) {
        var entries = loadImportDiagnostics()
        entries.removeAll { $0.id == entry.id }
        entries.insert(entry, at: 0)
        save(Array(entries.prefix(50)), forKey: Key.importDiagnostics)
    }

    func dismissImportDiagnosticsEntry(id: String) {
        var entries = loadImportDiagnostics()
        entries.removeAll { $0.id == id }
        save(entries, forKey: Key.importDiagnostics)
    }

    func loadPendingHomeImportPresentation() -> PendingHomeImportPresentation? {
        load(PendingHomeImportPresentation.self, forKey: Key.pendingHomeImportPresentation)
    }

    func savePendingHomeImportPresentation(_ presentation: PendingHomeImportPresentation) {
        save(presentation, forKey: Key.pendingHomeImportPresentation)
    }

    func clearPendingHomeImportPresentation() {
        defaults.removeObject(forKey: Key.pendingHomeImportPresentation)
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

    func loadLastKnownRider() -> CoordinatePoint? {
        load(CoordinatePoint.self, forKey: Key.lastKnownRider)
    }

    func saveLastKnownRider(_ point: CoordinatePoint) {
        save(point, forKey: Key.lastKnownRider)
    }

    func loadPairedPeripheral() -> PairedPeripheralRecord? {
        guard let data = defaults.data(forKey: Key.pairedPeripheral) else { return nil }
        return try? pairedPeripheralDecoder.decode(PairedPeripheralRecord.self, from: data)
    }

    func savePairedPeripheral(_ record: PairedPeripheralRecord) {
        guard let data = try? pairedPeripheralEncoder.encode(record) else { return }
        defaults.set(data, forKey: Key.pairedPeripheral)
    }

    func clearPairedPeripheral() {
        defaults.removeObject(forKey: Key.pairedPeripheral)
    }

    func loadRoutingDiagnosticsSessions() -> [RoutingDiagSession] {
        load([RoutingDiagSession].self, forKey: Key.routingDiagnostics) ?? []
    }

    func saveRoutingDiagnosticsSession(_ session: RoutingDiagSession) {
        var sessions = loadRoutingDiagnosticsSessions()
        sessions.removeAll { $0.id == session.id }
        sessions.insert(session, at: 0)
        save(Array(sessions.prefix(ROUTING_DIAGNOSTICS_SESSION_LIMIT)), forKey: Key.routingDiagnostics)
    }

    func dismissRoutingDiagnosticsSession(id: String) {
        var sessions = loadRoutingDiagnosticsSessions()
        sessions.removeAll { $0.id == id }
        save(sessions, forKey: Key.routingDiagnostics)
    }

    private func preferredDestinationTitle(newTitle: String, existingTitle: String) -> String {
        if isGenericDestinationTitle(existingTitle), !isGenericDestinationTitle(newTitle) {
            return newTitle
        }
        return existingTitle
    }

    private func isGenericDestinationTitle(_ title: String) -> Bool {
        let normalized = title.trimmingCharacters(in: .whitespacesAndNewlines).lowercased()
        return normalized.isEmpty || normalized == "dropped pin" || normalized == "recent destination" || normalized == "selected destination" || normalized == "route"
    }

    private func areNearby(_ lhs: CoordinatePoint, _ rhs: CoordinatePoint) -> Bool {
        PolylineGeo.straightLineMeters(lhs, rhs) <= nearbyDestinationMergeThresholdMeters
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
