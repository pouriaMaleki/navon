import Foundation

/// HSL/Digitransit availability checks extracted from AppModel.
enum HslAvailabilityService {
    /// Approximate bounding box for mainland Finland (including Åland). Digitransit's
    /// `finland` router aggregates GTFS feeds nationwide.
    static func isInFinland(_ point: CoordinatePoint) -> Bool {
        (59.7...70.1).contains(point.latitude) && (19.0...31.7).contains(point.longitude)
    }

    /// True when the user has enabled live HSL routing AND configured a Digitransit key.
    static func isHslLiveConfigured(settings: CompanionSettings) -> Bool {
        settings.preferLiveHslRouting
            && !settings.hslSubscriptionKey.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
    }

    /// True when both endpoints of the request fall inside Finland.
    static func isHslApplicableForRequest(_ request: RoutePlanRequest) -> Bool {
        isInFinland(request.origin) && isInFinland(request.destination)
    }

    static func isHslAvailable(settings: CompanionSettings, request: RoutePlanRequest) -> Bool {
        isHslLiveConfigured(settings: settings) && isHslApplicableForRequest(request)
    }

    static func sourceModeOptions(settings: CompanionSettings, request: RoutePlanRequest) -> [RouteSourceMode] {
        isHslAvailable(settings: settings, request: request) ? RouteSourceMode.allCases : [.osm]
    }

    /// When HSL becomes unusable (no key OR endpoints outside Finland), fall back any
    /// HSL-only or Mixed active selections to OSM. Persisted defaults are also normalised
    /// when the underlying *configuration* (the key) is gone, so a relaunch is consistent.
    static func normalizeSourceModeForHslAvailability(
        currentSourceMode: inout RouteSourceMode,
        settings: CompanionSettings,
        request: RoutePlanRequest,
        persistence: CompanionPersistence
    ) {
        if !isHslAvailable(settings: settings, request: request) && currentSourceMode != .osm {
            currentSourceMode = .osm
        }
        guard !isHslLiveConfigured(settings: settings) else { return }
        var preferences = persistence.loadRoutePlannerPreferences()
        if preferences.defaultSourceMode != .osm {
            preferences = RoutePlannerPreferences(
                defaultSourceMode: .osm,
                suggestionMode: preferences.suggestionMode,
                startBehavior: preferences.startBehavior
            )
            persistence.saveRoutePlannerPreferences(preferences)
        }
    }
}
