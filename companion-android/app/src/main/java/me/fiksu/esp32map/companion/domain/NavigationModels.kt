package me.fiksu.esp32map.companion.domain

enum class RouteSuggestionMode(val displayName: String) {
    BEST_ONLY("Best route"),
    THREE_ROUTES("3 suggestions"),
}

enum class RouteStartBehavior(val displayName: String) {
    EXPLICIT("Ask before start"),
    AUTOMATIC("Start automatically"),
}

data class RoutePlannerPreferences(
    val providerId: RouteProviderId = RouteProviderId.HSL,
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
