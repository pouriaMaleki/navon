package me.fiksu.esp32map.companion.feature.home

import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Job
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch
import me.fiksu.esp32map.companion.integration.share.UrlDestinationResolution
import me.fiksu.esp32map.companion.integration.share.resolveDestinationFromUrl
import me.fiksu.esp32map.companion.app.CompanionAppState
import me.fiksu.esp32map.companion.domain.CoordinatePoint
import me.fiksu.esp32map.companion.domain.DestinationSearchResult
import me.fiksu.esp32map.companion.domain.HomeCompassMode
import me.fiksu.esp32map.companion.domain.HomeMode
import me.fiksu.esp32map.companion.domain.NormalizedRoutePackage
import me.fiksu.esp32map.companion.domain.RouteAlternative
import me.fiksu.esp32map.companion.domain.RouteHistoryItem
import me.fiksu.esp32map.companion.domain.RouteHistorySource
import me.fiksu.esp32map.companion.domain.RouteManeuverType
import me.fiksu.esp32map.companion.domain.RoutePlanRequest
import me.fiksu.esp32map.companion.domain.RoutePreviewModel
import me.fiksu.esp32map.companion.domain.RouteProviderId
import me.fiksu.esp32map.companion.domain.RouteSourceMode
import me.fiksu.esp32map.companion.domain.RouteStartBehavior
import me.fiksu.esp32map.companion.domain.RouteSuggestionMode
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
    var homeMode by mutableStateOf(HomeMode.PLANNING)
    var compassMode by mutableStateOf(HomeCompassMode.AUTO_FOLLOW)
    /** True while a pasted URL (e.g. maps.app.goo.gl) is being followed to a destination. */
    var isResolvingUrl by mutableStateOf(false)
        private set
    /** Last URL-resolve failure for the search panel. */
    var urlResolveError by mutableStateOf<String?>(null)
        private set

    private var urlResolveJob: Job? = null

    private val switchablePlanningProviders = setOf(RouteProviderId.HSL, RouteProviderId.OSM)

    val sourceMode: RouteSourceMode
        get() = appState.currentSourceMode

    val recentItems: List<RouteHistoryItem>
        get() = appState.routeHistoryItems.take(visibleRecentCount)

    val visibleSuggestions: List<DestinationSearchResult>
        get() = suggestions.take(visibleSuggestionCount)

    val previewAlternatives: List<RouteAlternative>
        get() {
            val limit = if (appState.routePlannerPreferences.suggestionMode == RouteSuggestionMode.BEST_ONLY) 1 else 3
            return appState.preview.alternatives.take(limit)
        }

    val selectedPreview: RouteAlternative?
        get() = appState.preview.selectedAlternative

    val guidanceRoute: NormalizedRoutePackage?
        get() = when (homeMode) {
            HomeMode.PHONE_GUIDANCE, HomeMode.DEVICE_OVERVIEW, HomeMode.SENDING_TO_DEVICE -> selectedPreview?.normalizedPackage
            HomeMode.PLANNING -> null
        }

    val previewRoute: NormalizedRoutePackage?
        get() = selectedPreview?.normalizedPackage

    val destinationCoordinate: CoordinatePoint?
        get() = guidanceRoute?.geometry?.lastOrNull() ?: previewRoute?.geometry?.lastOrNull()

    val displayedRouteCoordinates: List<CoordinatePoint>
        get() = guidanceRoute?.geometry ?: previewRoute?.geometry ?: emptyList()

    val isPreviewLockedToImportedRoute: Boolean
        get() {
            val provider = previewRoute?.provenance?.providerId ?: return false
            return provider !in switchablePlanningProviders
        }

    val shouldShowSearchPanel: Boolean
        get() {
            if (homeMode != HomeMode.PLANNING || !isSearchOpen) return false
            if (isResolvingUrl || urlResolveError != null) return true
            return if (query.isBlank()) recentItems.isNotEmpty() else visibleSuggestions.isNotEmpty()
        }

    val shouldShowSourceControl: Boolean
        get() = homeMode == HomeMode.PLANNING
            && previewAlternatives.isNotEmpty()
            && !isPreviewLockedToImportedRoute
            && appState.sourceModeOptions.size > 1

    val routeSuggestionsTitle: String
        get() = if (isPreviewLockedToImportedRoute) "Imported route" else "Suggested routes"

    val isShowingActiveNavigation: Boolean
        get() = homeMode == HomeMode.PHONE_GUIDANCE || homeMode == HomeMode.DEVICE_OVERVIEW || homeMode == HomeMode.SENDING_TO_DEVICE

    val startButtonTitle: String
        get() = when (homeMode) {
            HomeMode.SENDING_TO_DEVICE -> "Starting on device..."
            HomeMode.PLANNING -> if (appState.isDeviceConnected) "Start on device" else "Start"
            HomeMode.PHONE_GUIDANCE, HomeMode.DEVICE_OVERVIEW -> "Start"
        }

    val activeNavigationTitle: String
        get() = guidanceRoute?.summary?.destinationLabel ?: appState.activeSession.destinationLabel

    val activeNavigationSubtitle: String
        get() = when (homeMode) {
            HomeMode.PHONE_GUIDANCE -> nextInstructionLine ?: "Riding on phone"
            HomeMode.DEVICE_OVERVIEW, HomeMode.SENDING_TO_DEVICE -> appState.syncSession.lastSyncResult
            HomeMode.PLANNING -> selectedPreview?.normalizedPackage?.summaryLine.orEmpty()
        }

        fun syncQueryFromPreview() {
        previewRoute?.summary?.destinationLabel?.takeIf { it.isNotBlank() }?.let {
            query = it
            return
        }
        selectedPreview?.title?.let { query = it }
    }

    val nextInstructionLine: String?
        get() {
            val route = guidanceRoute ?: return null
            val nextStep = route.maneuvers.firstOrNull { it.maneuverType != RouteManeuverType.DEPART }
            val instruction = nextStep?.instructionText ?: "Ride toward destination"
            return nextStep?.distanceFromStartMeters?.let { "$instruction • ${it.toInt()} m" } ?: instruction
        }

    fun openSearch() {
        if (homeMode != HomeMode.PLANNING) return
        isSearchOpen = true
        visibleRecentCount = 10
        visibleSuggestionCount = 10
    }

    fun closeSearch() {
        isSearchOpen = false
        urlResolveJob?.cancel()
        urlResolveJob = null
        isResolvingUrl = false
        urlResolveError = null
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
        if (homeMode != HomeMode.PLANNING) return
        query = newValue
        visibleSuggestionCount = 10
        val trimmed = newValue.trim()
        if (trimmed.isEmpty()) {
            suggestions = emptyList()
            urlResolveError = null
            urlResolveJob?.cancel()
            urlResolveJob = null
            isResolvingUrl = false
            return
        }
        if (trimmed.startsWith("http://", ignoreCase = true) || trimmed.startsWith("https://", ignoreCase = true)) {
            suggestions = emptyList()
            startUrlResolve(trimmed, scope)
            return
        }
        urlResolveError = null
        urlResolveJob?.cancel()
        urlResolveJob = null
        isResolvingUrl = false
        scope.launch {
            suggestions = placeSearchService.searchDestinations(newValue, 30)
        }
    }

    private fun startUrlResolve(url: String, scope: CoroutineScope) {
        urlResolveJob?.cancel()
        isResolvingUrl = true
        urlResolveError = null
        urlResolveJob = scope.launch {
            val resolution = resolveDestinationFromUrl(url)
            isResolvingUrl = false
            when (resolution) {
                is UrlDestinationResolution.Coordinate -> {
                    val title = resolution.suggestedTitle ?: "Imported destination"
                    appState.routeRequest = RoutePlanRequest(
                        origin = appState.riderLocation,
                        destination = resolution.point,
                        providerId = sourceMode.primaryProviderId,
                    )
                    closeSearch()
                    appState.planRoute(sourceMode = sourceMode, preferredTitle = title)
                    appState.recordRecentDestination(title, resolution.point)
                    appState.recordPlannedPreview(RouteHistorySource.PLANNED_ROUTE, sourceMode.displayName)
                    query = title
                }
                UrlDestinationResolution.NoDestinationFound -> {
                    urlResolveError = "Couldn't find a destination in that URL."
                }
                is UrlDestinationResolution.NetworkError -> {
                    urlResolveError = "URL expansion failed: ${resolution.message}"
                }
            }
        }
    }

    fun selectSuggestion(suggestion: DestinationSearchResult, scope: CoroutineScope) {
        closeSearch()
        appState.routeRequest = RoutePlanRequest(
            origin = appState.riderLocation,
            destination = suggestion.coordinate,
            providerId = sourceMode.primaryProviderId,
        )
        appState.recordRecentDestination(suggestion.title, suggestion.coordinate)
        appState.planRoute(sourceMode = sourceMode, preferredTitle = suggestion.title) {
            query = suggestion.title
            homeMode = HomeMode.PLANNING
            appState.recordPlannedPreview(RouteHistorySource.PLANNED_ROUTE, sourceMode.displayName)
            if (appState.routePlannerPreferences.startBehavior == RouteStartBehavior.AUTOMATIC) {
                scope.launch {
                    delay(150)
                    startSelectedRoute()
                }
            }
        }
    }

    fun selectRecent(item: RouteHistoryItem, scope: CoroutineScope) {
        closeSearch()
        appState.applyRouteHistoryPreview(item) {
            query = item.title
            homeMode = HomeMode.PLANNING
            if (appState.routePlannerPreferences.startBehavior == RouteStartBehavior.AUTOMATIC) {
                scope.launch {
                    delay(150)
                    startSelectedRoute()
                }
            }
        }
    }

    fun setDestinationFromMap(coordinate: CoordinatePoint, scope: CoroutineScope) {
        scope.launch {
            val resolved = placeSearchService.resolveDestination(coordinate, "Dropped pin")
            selectSuggestion(
                resolved ?: DestinationSearchResult(
                    id = "dropped-pin-${coordinate.latitude}-${coordinate.longitude}",
                    title = "Dropped pin",
                    subtitle = "Map destination",
                    coordinate = coordinate,
                ),
                scope,
            )
        }
    }

    fun setSourceMode(mode: RouteSourceMode) {
        if (!shouldShowSourceControl || mode == sourceMode) return
        appState.currentSourceMode = mode
        appState.saveRoutePlannerPreferences(appState.routePlannerPreferences.copy(defaultSourceMode = mode))
        val destination = destinationCoordinate ?: return
        appState.routeRequest = RoutePlanRequest(
            origin = appState.riderLocation,
            destination = destination,
            providerId = mode.primaryProviderId,
        )
        appState.planRoute(mode, preferredTitle = if (query.isBlank()) activeNavigationTitle else query) {
            appState.recordPlannedPreview(RouteHistorySource.PLANNED_ROUTE, mode.displayName)
        }
    }

    fun selectAlternative(alternativeId: String) {
        appState.selectAlternative(alternativeId)
    }

    fun startSelectedRoute() {
        closeSearch()
        val routeIdentifier = selectedPreview?.normalizedPackage?.routeIdentifier ?: return
        activeRouteIdentifier = routeIdentifier
        if (appState.isDeviceConnected) {
            homeMode = HomeMode.SENDING_TO_DEVICE
            appState.sendSelectedRoute { success ->
                homeMode = if (success) HomeMode.DEVICE_OVERVIEW else HomeMode.PLANNING
            }
        } else {
            compassMode = HomeCompassMode.AUTO_FOLLOW
            homeMode = HomeMode.PHONE_GUIDANCE
        }
    }

    fun stopActiveNavigation() {
        val destination = destinationCoordinate
        val sourceToReuse = appState.activeSession.sourceMode
        val shouldPreserveCurrentPreview = isPreviewLockedToImportedRoute
        compassMode = HomeCompassMode.AUTO_FOLLOW
        activeRouteIdentifier = null
        if (homeMode == HomeMode.DEVICE_OVERVIEW || homeMode == HomeMode.SENDING_TO_DEVICE) {
            appState.clearActiveRoute()
        }
        homeMode = HomeMode.PLANNING
        if (shouldPreserveCurrentPreview || destination == null) {
            return
        }
        appState.routeRequest = RoutePlanRequest(
            origin = appState.riderLocation,
            destination = destination,
            providerId = sourceToReuse.primaryProviderId,
        )
        appState.planRoute(sourceToReuse, preferredTitle = if (query.isBlank()) activeNavigationTitle else query) {
            appState.recordPlannedPreview(RouteHistorySource.PLANNED_ROUTE, sourceToReuse.displayName)
        }
    }

    fun clearPreview() {
        activeRouteIdentifier = null
        homeMode = HomeMode.PLANNING
        compassMode = HomeCompassMode.AUTO_FOLLOW
        query = ""
        suggestions = emptyList()
        closeSearch()
        appState.preview = RoutePreviewModel()
    }

    fun handleCompassTap(scope: CoroutineScope) {
        if (homeMode != HomeMode.PHONE_GUIDANCE) return
        when (compassMode) {
            HomeCompassMode.AUTO_FOLLOW -> {
                compassMode = HomeCompassMode.NORTH_PREVIEW
                scope.launch {
                    delay(2500)
                    if (homeMode == HomeMode.PHONE_GUIDANCE && compassMode == HomeCompassMode.NORTH_PREVIEW) {
                        compassMode = HomeCompassMode.AUTO_FOLLOW
                    }
                }
            }
            HomeCompassMode.NORTH_PREVIEW -> compassMode = HomeCompassMode.NORTH_LOCKED
            HomeCompassMode.NORTH_LOCKED -> compassMode = HomeCompassMode.AUTO_FOLLOW
        }
    }

    fun handleCompassDoubleTap() {
        if (homeMode != HomeMode.PHONE_GUIDANCE) return
        compassMode = HomeCompassMode.NORTH_LOCKED
    }
}
