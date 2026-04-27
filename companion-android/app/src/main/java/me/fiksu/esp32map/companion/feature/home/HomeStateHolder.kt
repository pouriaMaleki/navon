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
import me.fiksu.esp32map.companion.integration.location.HeadingTrail

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
    /**
     * Monotonic counter observed by the Compose map. Bumped on every
     * compass-tap (spec line 39) AND after the inactivity timeout following
     * a user map interaction during routing (spec line 104).
     */
    var mapRecenterRequestTick by mutableIntStateOf(0)
        private set
    /**
     * Monotonic counter observed by the Compose map. Bumped on every
     * rider-location update during routing so the camera follows the rider
     * (spec line 84).
     */
    var mapFollowRiderTick by mutableIntStateOf(0)
        private set

    private var urlResolveJob: Job? = null
    private var mapInteractionRecenterJob: Job? = null
    /**
     * Spec line 110 (authoritative): smoothed travel heading from the last
     * few GPS fixes. When available, overrides the route-segment bearing
     * so the camera rotates to the rider's actual direction of travel.
     * Parameters match runtime-core / companion-web / companion-ios.
     */
    private val headingTrail = HeadingTrail(
        maxAgeMs = 5_000L, maxFixes = 10,
        minDisplacementM = 3.0, smoothingAlpha = 0.25,
    )
    /**
     * Pinned auto-recenter delay for user map interactions during routing.
     * Mirrors `recenter_inactivity_ms` in parity-fixtures/data/ux-constants.toml
     * (spec line 104).
     */
    private val mapInteractionRecenterDelayMs = 1300L

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
            HomeMode.PHONE_GUIDANCE -> {
                // Mirror web/iOS: "X km remaining • Y min" — but without
                // route-progress tracking on Android we substitute the
                // route summary line. Still gives the rider a sense of
                // total distance / ETA.
                guidanceRoute?.summaryLine ?: "Riding on phone"
            }
            HomeMode.DEVICE_OVERVIEW, HomeMode.SENDING_TO_DEVICE -> appState.syncSession.lastSyncResult
            HomeMode.PLANNING -> selectedPreview?.normalizedPackage?.summaryLine.orEmpty()
        }

    /**
     * Single line: destination address + remaining distance + remaining
     * minutes. Pinned as the subtitle of the top guidance card so the bottom
     * no longer needs to repeat the same information; it shows only a
     * floating Stop button. Empty destination labels are dropped so the line
     * never starts with a stray separator.
     */
    val guidanceSubtitleLine: String
        get() {
            val destination = (guidanceRoute?.summary?.destinationLabel ?: appState.activeSession.destinationLabel).trim()
            val remainingPart = activeNavigationSubtitle
            return if (destination.isEmpty() || destination == "No destination") {
                remainingPart
            } else {
                "$destination • $remainingPart"
            }
        }

    /**
     * Set when the rider arrives at the destination; cleared when the next
     * route starts. The Compose UI shows a banner in the bottom slot and
     * routing has been auto-stopped.
     */
    var arrivalNotice by mutableStateOf<String?>(null)
        private set

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
            // Spec line 75: bias typeahead toward the rider's area.
            val bias = appState.locationState.currentLocation
                ?: appState.locationState.lastKnownLocation
            suggestions = placeSearchService.searchDestinations(newValue, 30, bias)
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
        // Starting a new route clears any stale arrival banner from the
        // previous trip.
        arrivalNotice = null
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
        // Spec line 39: companion compass tap also recenters the camera.
        mapRecenterRequestTick += 1
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

    /**
     * Bearing (clockwise from true north, degrees) of the route segment
     * that `rider` is currently projected onto. Spec line 101: camera rotates
     * so the immediate route direction is toward the top of the screen,
     * "riding towards, even when stationary yet". Falls back to the first
     * leg when `rider` is at the start.
     */
    fun routingBearingDegrees(rider: CoordinatePoint): Double {
        val geometry = guidanceRoute?.geometry ?: previewRoute?.geometry ?: emptyList()
        if (geometry.size < 2) return 0.0
        val metersPerDegreeLat = 111_320.0
        fun segLen(a: CoordinatePoint, b: CoordinatePoint): Double {
            val latMeters = (b.latitude - a.latitude) * metersPerDegreeLat
            val meanLat = ((a.latitude + b.latitude) / 2.0) * Math.PI / 180.0
            val lonMeters = (b.longitude - a.longitude) * kotlin.math.cos(meanLat) * metersPerDegreeLat
            return kotlin.math.sqrt(latMeters * latMeters + lonMeters * lonMeters)
        }
        fun segBearing(a: CoordinatePoint, b: CoordinatePoint): Double {
            val latMeters = (b.latitude - a.latitude) * metersPerDegreeLat
            val meanLat = ((a.latitude + b.latitude) / 2.0) * Math.PI / 180.0
            val lonMeters = (b.longitude - a.longitude) * kotlin.math.cos(meanLat) * metersPerDegreeLat
            return kotlin.math.atan2(lonMeters, latMeters) * 180.0 / Math.PI
        }
        // Project rider onto polyline to find progress distance.
        var bestDistSq = Double.POSITIVE_INFINITY
        var bestProgress = 0.0
        var walked = 0.0
        for (i in 0 until geometry.size - 1) {
            val a = geometry[i]
            val b = geometry[i + 1]
            val meanLat = ((a.latitude + rider.latitude) / 2.0) * Math.PI / 180.0
            val cosLat = kotlin.math.cos(meanLat)
            val endX = (b.longitude - a.longitude) * cosLat * metersPerDegreeLat
            val endY = (b.latitude - a.latitude) * metersPerDegreeLat
            val riderX = (rider.longitude - a.longitude) * cosLat * metersPerDegreeLat
            val riderY = (rider.latitude - a.latitude) * metersPerDegreeLat
            val lenSq = endX * endX + endY * endY
            if (lenSq < 1e-12) continue
            val t = ((riderX * endX + riderY * endY) / lenSq).coerceIn(0.0, 1.0)
            val dx = riderX - t * endX
            val dy = riderY - t * endY
            val distSq = dx * dx + dy * dy
            val len = kotlin.math.sqrt(lenSq)
            if (distSq < bestDistSq) {
                bestDistSq = distSq
                bestProgress = walked + len * t
            }
            walked += len
        }
        // Find the segment containing `bestProgress` with strict `<` so a
        // rider at a vertex snaps to the NEXT segment (riding towards).
        var traversed = 0.0
        for (i in 0 until geometry.size - 1) {
            val len = segLen(geometry[i], geometry[i + 1])
            if (len < 1e-6) continue
            if (bestProgress < traversed + len) {
                return segBearing(geometry[i], geometry[i + 1])
            }
            traversed += len
        }
        return segBearing(geometry[geometry.size - 2], geometry[geometry.size - 1])
    }

    /**
     * Called on every new rider-location update. During phone guidance this
     * bumps `mapFollowRiderTick` so the Compose map re-anchors on the rider
     * (spec line 84). Outside phoneGuidance it's a no-op.
     */
    fun notifyRiderLocationUpdated() {
        // Spec line 84 + 108-118: bump on every fix in routing OR when the
        // rider is moving in any mode. The Compose camera onChange respects
        // mode + trail in its dispatch.
        val moving = travelHeadingDegrees != null
        if (homeMode != HomeMode.PHONE_GUIDANCE && !moving) return
        mapFollowRiderTick += 1
    }

    /**
     * Compose-observable cache of `headingTrail.travelHeadingDegrees`. The
     * raw trail is a plain object with no Compose hookup; this mirror is
     * updated on every `ingestRiderLocationFix` so LaunchedEffects can
     * react to "rider started/stopped moving" without polling.
     */
    private var trailHeadingCache by mutableStateOf<Double?>(null)

    /**
     * Feed a GPS fix into the heading-trail buffer and the Compose-observable
     * mirror. Drives spec line 110 (GPS-derived camera rotation). Callable
     * from the location listener AND from tests.
     */
    fun ingestRiderLocationFix(point: CoordinatePoint, timestampMs: Long) {
        headingTrail.recordFix(point, timestampMs)
        trailHeadingCache = headingTrail.travelHeadingDegrees
        // Spec: when the rider arrives at the destination, end routing.
        // Straight-line distance to the route's last vertex — Android lacks
        // along-route projection, so this check trips when the rider is
        // physically near the destination from any approach direction.
        if (homeMode == HomeMode.PHONE_GUIDANCE) {
            val destination = guidanceRoute?.geometry?.lastOrNull()
            if (destination != null && straightLineMeters(destination, point) <= ARRIVAL_RADIUS_M) {
                declareArrival()
            }
        }
    }

    private fun declareArrival() {
        arrivalNotice = "Arrived at destination"
        // Reuse the manual-stop teardown so persistence + UI stay consistent.
        // arrivalNotice survives because stopActiveNavigation() doesn't clear it.
        stopActiveNavigation()
    }

    companion object {
        /**
         * Larger than the off-route exit distance so a rider drifting around
         * the destination doesn't bounce between "off route" and "arrived".
         */
        private const val ARRIVAL_RADIUS_M = 25.0

        private fun straightLineMeters(a: CoordinatePoint, b: CoordinatePoint): Double {
            val metersPerDegLat = 111_320.0
            val dN = (b.latitude - a.latitude) * metersPerDegLat
            val meanLat = ((a.latitude + b.latitude) / 2.0) * Math.PI / 180.0
            val dE = (b.longitude - a.longitude) * kotlin.math.cos(meanLat) * metersPerDegLat
            return kotlin.math.sqrt(dN * dN + dE * dE)
        }
    }

    /**
     * Smoothed travel heading from the last few GPS fixes, `null` while
     * stationary. Spec line 110: this overrides the route-segment bearing
     * when the rider is moving.
     */
    val travelHeadingDegrees: Double?
        get() = trailHeadingCache

    /**
     * Unified camera heading merging spec lines 110 (GPS trail, wins when
     * moving) and 101 (route segment, fallback when stationary). Used by
     * the Compose map's `orientCameraForTravel` path.
     */
    fun cameraHeadingDegrees(rider: CoordinatePoint?): Double {
        headingTrail.travelHeadingDegrees?.let { return it }
        return rider?.let(::routingBearingDegrees) ?: 0.0
    }

    /**
     * Called by the Compose map on every user pan/zoom/rotate during
     * routing. Schedules a recenter to the routing default after the pinned
     * inactivity timeout (spec line 104). Outside phoneGuidance it's a
     * no-op. Successive interactions reset the timer.
     */
    fun noteUserMapInteraction(scope: CoroutineScope) {
        if (homeMode != HomeMode.PHONE_GUIDANCE) return
        mapInteractionRecenterJob?.cancel()
        mapInteractionRecenterJob = scope.launch {
            delay(mapInteractionRecenterDelayMs)
            if (homeMode == HomeMode.PHONE_GUIDANCE) {
                mapRecenterRequestTick += 1
            }
        }
    }
}
