package me.fiksu.esp32map.companion.feature.home

import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch
import me.fiksu.esp32map.companion.app.CompanionAppState
import me.fiksu.esp32map.companion.domain.CoordinatePoint
import me.fiksu.esp32map.companion.domain.DestinationSearchResult
import me.fiksu.esp32map.companion.domain.NormalizedRoutePackage
import me.fiksu.esp32map.companion.domain.RouteAlternative
import me.fiksu.esp32map.companion.domain.RouteHistoryItem
import me.fiksu.esp32map.companion.domain.RoutePlanRequest
import me.fiksu.esp32map.companion.integration.PlaceSearchService

class HomeStateHolder(
    private val appState: CompanionAppState,
    private val placeSearchService: PlaceSearchService,
) {
    var query by mutableStateOf("")
    var isSearchOpen by mutableStateOf(false)
    var suggestions by mutableStateOf<List<DestinationSearchResult>>(emptyList())
    var visibleSuggestionCount by mutableIntStateOf(10)
    var visibleRecentCount by mutableIntStateOf(10)
    var activeRouteIdentifier by mutableStateOf<String?>(null)

    val recentItems: List<RouteHistoryItem>
        get() = appState.routeHistoryItems.take(visibleRecentCount)

    val visibleSuggestions: List<DestinationSearchResult>
        get() = suggestions.take(visibleSuggestionCount)

    val previewAlternatives: List<RouteAlternative>
        get() = when (appState.routePlannerPreferences.suggestionMode) {
            me.fiksu.esp32map.companion.domain.RouteSuggestionMode.BEST_ONLY -> appState.preview.alternatives.take(1)
            me.fiksu.esp32map.companion.domain.RouteSuggestionMode.THREE_ROUTES -> appState.preview.alternatives.take(3)
        }

    val selectedPreview: RouteAlternative?
        get() = appState.preview.selectedAlternative

    val activeRoute: NormalizedRoutePackage?
        get() = if (activeRouteIdentifier != null) selectedPreview?.normalizedPackage else null

    val previewRoute: NormalizedRoutePackage?
        get() = selectedPreview?.normalizedPackage

    val destinationCoordinate: CoordinatePoint?
        get() = activeRoute?.geometry?.lastOrNull() ?: previewRoute?.geometry?.lastOrNull()

    fun openSearch() {
        isSearchOpen = true
        visibleRecentCount = 10
        visibleSuggestionCount = 10
    }

    fun closeSearch() {
        isSearchOpen = false
    }

    fun loadMoreRecentsIfNeeded(item: RouteHistoryItem) {
        if (item.id == recentItems.lastOrNull()?.id) {
            visibleRecentCount += 10
        }
    }

    fun loadMoreSuggestionsIfNeeded(item: DestinationSearchResult) {
        if (item.id == visibleSuggestions.lastOrNull()?.id) {
            visibleSuggestionCount += 10
        }
    }

    fun updateQuery(newValue: String, scope: CoroutineScope) {
        query = newValue
        visibleSuggestionCount = 10
        if (newValue.isBlank()) {
            suggestions = emptyList()
            return
        }
        scope.launch {
            suggestions = placeSearchService.searchDestinations(newValue, 30)
        }
    }

    fun selectSuggestion(suggestion: DestinationSearchResult, scope: CoroutineScope) {
        scope.launch {
            val providerId = appState.routePlannerPreferences.providerId
            appState.selectedProviderId = providerId
            appState.routeRequest = RoutePlanRequest(
                origin = appState.simulatedRiderLocation,
                destination = suggestion.coordinate,
                providerId = providerId,
            )
            appState.planRoute()
            query = suggestion.title
            closeSearch()
            if (appState.routePlannerPreferences.startBehavior == me.fiksu.esp32map.companion.domain.RouteStartBehavior.AUTOMATIC) {
                delay(150)
                startSelectedRoute()
            }
        }
    }

    fun selectRecent(item: RouteHistoryItem, scope: CoroutineScope) {
        scope.launch {
            if (item.routePackage != null) {
                val packageRef = item.routePackage
                appState.preview = appState.preview.copy(
                    alternatives = listOf(
                        RouteAlternative(
                            id = item.id,
                            title = item.title,
                            subtitle = item.subtitle,
                            distanceMeters = packageRef.summary.totalDistanceMeters.toInt(),
                            durationSeconds = packageRef.summary.estimatedDurationSeconds,
                            normalizedPackage = packageRef,
                        ),
                    ),
                    selectedAlternativeId = item.id,
                    routeIdentifier = packageRef.routeIdentifier,
                    routeRevision = packageRef.revision,
                    planningNotice = item.sourceLabel,
                )
            } else if (item.destination != null) {
                val providerId = appState.routePlannerPreferences.providerId
                appState.selectedProviderId = providerId
                appState.routeRequest = RoutePlanRequest(
                    origin = appState.simulatedRiderLocation,
                    destination = item.destination,
                    providerId = providerId,
                )
                appState.planRoute()
            }
            closeSearch()
        }
    }

    fun setDestinationFromMap(coordinate: CoordinatePoint, scope: CoroutineScope) {
        selectSuggestion(
            DestinationSearchResult(
                id = "dropped-pin-${coordinate.latitude}-${coordinate.longitude}",
                title = "Dropped pin",
                subtitle = "Map destination",
                coordinate = coordinate,
            ),
            scope,
        )
    }

    fun selectAlternative(alternativeId: String) {
        appState.selectAlternative(alternativeId)
    }

    fun startSelectedRoute() {
        activeRouteIdentifier = selectedPreview?.normalizedPackage?.routeIdentifier
    }

    fun stopGuidance(scope: CoroutineScope) {
        activeRouteIdentifier = null
        val destination = destinationCoordinate ?: return
        scope.launch {
            val providerId = appState.routePlannerPreferences.providerId
            appState.selectedProviderId = providerId
            appState.routeRequest = RoutePlanRequest(
                origin = appState.simulatedRiderLocation,
                destination = destination,
                providerId = providerId,
            )
            appState.planRoute()
        }
    }
}
