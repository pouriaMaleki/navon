package me.fiksu.esp32map.companion.domain

enum class RouteSuggestionMode(val displayName: String) {
    BEST_ONLY("Best route"),
    THREE_ROUTES("3 suggestions"),
}

enum class RouteStartBehavior(val displayName: String) {
    EXPLICIT("Ask before start"),
    AUTOMATIC("Start automatically"),
}

enum class RouteSourceMode(val displayName: String, val primaryProviderId: RouteProviderId, val providerIds: List<RouteProviderId>) {
    MIXED("Mixed", RouteProviderId.HSL, listOf(RouteProviderId.HSL, RouteProviderId.OSM)),
    HSL("HSL", RouteProviderId.HSL, listOf(RouteProviderId.HSL)),
    OSM("OSM", RouteProviderId.OSM, listOf(RouteProviderId.OSM)),
}

enum class RouteSuggestionKind(val displayName: String) {
    FASTEST("Fastest"),
    QUIETER("Quieter"),
    SIMPLER("Simpler"),
}

enum class HomeMode {
    PLANNING,
    SENDING_TO_DEVICE,
    PHONE_GUIDANCE,
    DEVICE_OVERVIEW,
}

enum class HomeCompassMode {
    AUTO_FOLLOW,
    NORTH_PREVIEW,
    NORTH_LOCKED,
}

data class RoutePlannerPreferences(
    val defaultSourceMode: RouteSourceMode = RouteSourceMode.MIXED,
    val suggestionMode: RouteSuggestionMode = RouteSuggestionMode.THREE_ROUTES,
    val startBehavior: RouteStartBehavior = RouteStartBehavior.EXPLICIT,
)

data class DestinationSearchResult(
    val id: String,
    val title: String,
    val subtitle: String,
    val coordinate: CoordinatePoint,
)

enum class RouteHistorySource {
    RECENT_DESTINATION,
    PLANNED_ROUTE,
    GPX_IMPORT,
    GOOGLE_MAPS,
    PARTNER,
}

data class RouteHistoryItem(
    val id: String,
    val title: String,
    val subtitle: String,
    val source: RouteHistorySource,
    val sourceLabel: String,
    val createdAtLabel: String,
    val destination: CoordinatePoint?,
    val routePackage: NormalizedRoutePackage?,
)
