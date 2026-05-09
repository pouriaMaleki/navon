package me.fiksu.esp32map.companion.feature.home

import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Job
import kotlinx.coroutines.MainScope
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
import me.fiksu.esp32map.companion.domain.RerouteContext
import me.fiksu.esp32map.companion.domain.RouteSourceMode
import me.fiksu.esp32map.companion.domain.RouteStartBehavior
import me.fiksu.esp32map.companion.domain.RouteSuggestionMode
import me.fiksu.esp32map.companion.integration.PlaceSearchService
import me.fiksu.esp32map.companion.integration.location.HeadingTrail

class HomeStateHolder(
    private val appState: CompanionAppState,
    private val placeSearchService: PlaceSearchService,
    private val autoRerouteDispatcher: suspend (CoordinatePoint) -> Unit = { rider ->
        appState.rerouteActiveSession(
            rider,
            "Off-route",
            RerouteContext(
                headingDegrees = travelHeadingDegrees,
                speedMps = appState.locationState.currentSpeedMps,
            ),
        )
    },
    private val autoRerouteScope: CoroutineScope = MainScope(),
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
    /**
     * True while the rider is browsing alternative routes from active guidance
     * (the "split icon" flow). `homeMode` stays `PHONE_GUIDANCE` so guidance
     * keeps running; the alternatives card is shown via this flag.
     */
    var isExploringAlternativesFromGuidance by mutableStateOf(false)
        private set
    /** The alternative explicitly tapped by the user during exploration. Null on enter. */
    var explorationSelectedID: String? by mutableStateOf(null)
        private set
    /** The route package that was active when guidance started; frozen during exploration. */
    private var activeRoutePackage: NormalizedRoutePackage? by mutableStateOf(null)

    private var urlResolveJob: Job? = null
    private var mapInteractionRecenterJob: Job? = null
    /**
     * Spec line 110 (authoritative): smoothed travel heading from the last
     * few GPS fixes. When available, overrides the route-segment bearing
     * so the camera rotates to the rider's actual direction of travel.
     *
     * Tuned for cycling responsiveness: at α=0.25 / 5 s buffer the
     * smoother takes ~5 s to land within 10° of a new heading after a
     * sharp turn — by which time the rider is past the corner and the
     * camera feels laggy. α=0.45 / 3 s halves the lag while still
     * absorbing GPS jitter (raised the displacement floor to 4 m so
     * stationary-mode jitter doesn't produce a phantom heading).
     */
    private val headingTrail = HeadingTrail(
        maxAgeMs = 3_000L, maxFixes = 6,
        minDisplacementM = 4.0, smoothingAlpha = 0.45,
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
            HomeMode.PHONE_GUIDANCE -> {
                if (isExploringAlternativesFromGuidance) activeRoutePackage
                else selectedPreview?.normalizedPackage
            }
            HomeMode.DEVICE_OVERVIEW, HomeMode.SENDING_TO_DEVICE -> selectedPreview?.normalizedPackage
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
        get() {
            val placeholders = setOf("selected destination", "current location")
            val routeLabel = guidanceRoute?.summary?.destinationLabel?.trim() ?: ""
            if (routeLabel.isNotEmpty() && !placeholders.contains(routeLabel.lowercase())) return routeLabel
            return appState.activeSession.destinationLabel
        }

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
            val placeholders = setOf("", "no destination", "selected destination", "current location")
            val destination = (guidanceRoute?.summary?.destinationLabel ?: appState.activeSession.destinationLabel).trim()
            val remainingPart = activeNavigationSubtitle
            return if (placeholders.contains(destination.lowercase())) {
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

    /**
     * Auto-dismiss the arrival banner after 60 s so a rider who walked away
     * from the phone doesn't return to a banner blocking new route
     * suggestions. Mirrors web/iOS.
     */
    private val arrivalNoticeAutoDismissMs: Long = 60_000L
    private var arrivalNoticeAutoDismissJob: Job? = null
    private val arrivalNoticeScope: CoroutineScope = MainScope()

    /**
     * Distance (m) the rider has progressed along the active route, from
     * monotonic projection of each fix onto the polyline. Bumped only on
     * `ingestRiderLocationFix` while in PHONE_GUIDANCE. Drives the
     * route-overview camera (#7) so the compass-tap fit ignores already-
     * completed segments behind the rider.
     */
    var progressDistanceM by mutableStateOf(0.0)
        private set
    private var routeTotalDistanceM: Double = 0.0
    var offRouteDistanceM by mutableStateOf(0.0)
        private set
    var offRoute by mutableStateOf(false)
        private set
    var rerouteRequested by mutableStateOf(false)
        private set
    private var offRouteDurationMs: Double = 0.0
    private var lastAdvanceTimestampMs: Long = -1
    private var autoReroutePending: Boolean = false
    private var pendingAutoRerouteJob: Job? = null
    private var reroutingDelayedUntilMs: Double? = null
    private val reroutingAttemptTimestamps = mutableListOf<Double>()

    /**
     * Geometry the route-overview camera should fit when the user taps
     * the compass during routing. Equals the remaining route ahead of
     * the rider — late in a ride the start segment is no longer relevant,
     * so including it would zoom the camera out unnecessarily. Falls back
     * to the full geometry before any progress.
     */
    val routeOverviewGeometry: List<CoordinatePoint>
        get() {
            val geometry = guidanceRoute?.geometry ?: return emptyList()
            if (geometry.isEmpty()) return emptyList()
            val split = splitPolylineAtDistance(geometry, progressDistanceM)
            return if (split.remaining.size >= 2) split.remaining else geometry
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
        // previous trip and resets progress so route-overview-remaining
        // begins at the start of the new geometry.
        cancelArrivalNoticeAutoDismiss()
        arrivalNotice = null
        progressDistanceM = 0.0
        routeTotalDistanceM = polylineLengthMeters(selectedPreview?.normalizedPackage?.geometry ?: emptyList())
        offRouteDistanceM = 0.0
        offRoute = false
        rerouteRequested = false
        offRouteDurationMs = 0.0
        lastAdvanceTimestampMs = -1
        autoReroutePending = false
        pendingAutoRerouteJob?.cancel()
        pendingAutoRerouteJob = null
        reroutingDelayedUntilMs = null
        reroutingAttemptTimestamps.clear()
        if (appState.isDeviceConnected) {
            homeMode = HomeMode.SENDING_TO_DEVICE
            appState.sendSelectedRoute { success ->
                homeMode = if (success) HomeMode.DEVICE_OVERVIEW else HomeMode.PLANNING
            }
        } else {
            compassMode = HomeCompassMode.AUTO_FOLLOW
            isExploringAlternativesFromGuidance = false
            explorationSelectedID = null
            activeRoutePackage = selectedPreview?.normalizedPackage
            homeMode = HomeMode.PHONE_GUIDANCE
        }
    }

    /**
     * Spec #11 ("split-way reroute"): enter the "browse alternatives from
     * guidance" state. `homeMode` stays `PHONE_GUIDANCE` so guidance keeps
     * running; the alternatives card is shown via `isExploringAlternativesFromGuidance`.
     * Re-plans from the rider's current location to the active destination so
     * fresh alternatives are available to pick from.
     */
    fun exploreAlternateRoutes(scope: CoroutineScope) {
        if (homeMode != HomeMode.PHONE_GUIDANCE) return
        val destination = destinationCoordinate ?: return
        val sourceToReuse = appState.activeSession.sourceMode
        val titleHint = appState.activeSession.destinationLabel
        // Set the flag immediately so the alternatives panel opens right away
        // with a loading indicator while the plan fetches in the background.
        isExploringAlternativesFromGuidance = true
        explorationSelectedID = null
        compassMode = HomeCompassMode.NORTH_LOCKED
        appState.routeRequest = RoutePlanRequest(
            origin = appState.riderLocation,
            destination = destination,
            providerId = sourceToReuse.primaryProviderId,
        )
        scope.launch {
            appState.planRoute(sourceToReuse, preferredTitle = if (titleHint.isBlank()) null else titleHint)
        }
    }

    /**
     * Cancel browsing alternatives — dismiss the card, resume normal routing UI.
     * The original route is still active; camera returns to autoFollow.
     */
    fun cancelAlternativesExploration() {
        isExploringAlternativesFromGuidance = false
        explorationSelectedID = null
        compassMode = HomeCompassMode.AUTO_FOLLOW
    }

    /**
     * Clear the explicitly-tapped alternative selection so the checkmark
     * moves back to "Continue on current route". Does NOT exit exploration.
     */
    fun deselectForExploration() {
        if (!isExploringAlternativesFromGuidance) return
        explorationSelectedID = null
    }

    /**
     * Select an alternative for preview while exploring from guidance.
     * Records the explicit user tap so selectedAlternativeIdForDisplay shows the checkmark.
     */
    fun selectAlternativeForExploration(id: String) {
        explorationSelectedID = id
        appState.selectAlternativePreviewOnly(id)
    }

    /**
     * The alternative ID that should show a checkmark in the suggestions card.
     * During exploration, returns the ID explicitly tapped by the user (null
     * until first tap). Outside exploration, returns the planning-selected ID.
     */
    val selectedAlternativeIdForDisplay: String?
        get() = if (isExploringAlternativesFromGuidance) explorationSelectedID else appState.preview.selectedAlternativeId

    /**
     * Alternative routes to render on the map during exploration.
     * Returns an empty list outside of exploration.
     */
    val guidanceAlternatives: List<RouteAlternative>
        get() = if (isExploringAlternativesFromGuidance) appState.preview.alternatives else emptyList()

    private fun polylineLengthMeters(polyline: List<CoordinatePoint>): Double {
        if (polyline.size < 2) return 0.0
        val metersPerDegLat = 111_320.0
        var total = 0.0
        for (i in 0 until polyline.size - 1) {
            val a = polyline[i]
            val b = polyline[i + 1]
            val meanLat = ((a.latitude + b.latitude) / 2.0) * Math.PI / 180.0
            val dN = (b.latitude - a.latitude) * metersPerDegLat
            val dE = (b.longitude - a.longitude) * kotlin.math.cos(meanLat) * metersPerDegLat
            total += kotlin.math.sqrt(dN * dN + dE * dE)
        }
        return total
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
        offRouteDistanceM = 0.0
        offRoute = false
        rerouteRequested = false
        offRouteDurationMs = 0.0
        lastAdvanceTimestampMs = -1
        autoReroutePending = false
        pendingAutoRerouteJob?.cancel()
        pendingAutoRerouteJob = null
        reroutingDelayedUntilMs = null
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
        if (homeMode == HomeMode.PHONE_GUIDANCE) {
            // Project onto active geometry and advance progress monotonically
            // (never regresses) so route-overview-remaining (#7) can drop the
            // completed prefix.
            val geometry = guidanceRoute?.geometry
            if (geometry != null && geometry.size >= 2) {
                val projection = projectProgressWithDistance(geometry, point)
                val capped = if (routeTotalDistanceM > 0.0) {
                    kotlin.math.min(routeTotalDistanceM, projection.progressM)
                } else projection.progressM
                if (capped > progressDistanceM) progressDistanceM = capped
                offRouteDistanceM = projection.distanceToRouteM

                val dt = if (lastAdvanceTimestampMs >= 0) {
                    (timestampMs - lastAdvanceTimestampMs).toDouble()
                } else 0.0
                lastAdvanceTimestampMs = timestampMs
                val wasOffRoute = offRoute
                val prevRerouteRequested = rerouteRequested
                if (offRoute) {
                    if (projection.distanceToRouteM <= OFF_ROUTE_EXIT_DISTANCE_M) offRoute = false
                } else if (projection.distanceToRouteM >= OFF_ROUTE_ENTER_DISTANCE_M) {
                    offRoute = true
                }
                if (offRoute) {
                    offRouteDurationMs = if (wasOffRoute) offRouteDurationMs + dt else dt
                    if (offRouteDurationMs >= REROUTE_REQUEST_DELAY_MS) rerouteRequested = true
                } else {
                    offRouteDurationMs = 0.0
                    rerouteRequested = false
                    autoReroutePending = false
                }
                if (rerouteRequested && !prevRerouteRequested && !autoReroutePending) {
                    autoReroutePending = true
                    val delayMs = recordReroutingAttempt(timestampMs.toDouble())
                    pendingAutoRerouteJob = autoRerouteScope.launch {
                        if (delayMs > 0.0) {
                            while (true) {
                                val until = reroutingDelayedUntilMs
                                val now = System.currentTimeMillis().toDouble()
                                if (until == null || now >= until) break
                                delay(250)
                            }
                        }
                        markAutoRerouteDispatched()
                        try {
                            autoRerouteDispatcher(point)
                        } finally {
                            autoReroutePending = false
                        }
                    }
                }
            }
            // Spec: when the rider arrives at the destination, end routing.
            // Straight-line distance to the route's last vertex so a rider
            // approaching from any side trips arrival.
            val destination = geometry?.lastOrNull()
            if (destination != null && straightLineMeters(destination, point) <= ARRIVAL_RADIUS_M) {
                declareArrival()
            }
        }
    }

    /**
     * Distance along `polyline` to the closest projected point on `rider`.
     * Mirrors the iOS `projectProgress` helper. Equirectangular metric — the
     * absolute scale is approximate but the relative ordering is exact, which
     * is what the monotonic-progress invariant needs.
     */
    private fun projectProgressMeters(polyline: List<CoordinatePoint>, rider: CoordinatePoint): Double {
        return projectProgressWithDistance(polyline, rider).progressM
    }

    private data class ProgressProjection(
        val progressM: Double,
        val distanceToRouteM: Double,
    )

    private fun projectProgressWithDistance(polyline: List<CoordinatePoint>, rider: CoordinatePoint): ProgressProjection {
        if (polyline.size < 2) return ProgressProjection(0.0, 0.0)
        val metersPerDegLat = 111_320.0
        var bestDistSq = Double.POSITIVE_INFINITY
        var bestProgress = 0.0
        var traversed = 0.0
        for (i in 0 until polyline.size - 1) {
            val a = polyline[i]
            val b = polyline[i + 1]
            val meanLat = ((a.latitude + rider.latitude) / 2.0) * Math.PI / 180.0
            val cosLat = kotlin.math.cos(meanLat)
            val endX = (b.longitude - a.longitude) * cosLat * metersPerDegLat
            val endY = (b.latitude - a.latitude) * metersPerDegLat
            val riderX = (rider.longitude - a.longitude) * cosLat * metersPerDegLat
            val riderY = (rider.latitude - a.latitude) * metersPerDegLat
            val segLenSq = endX * endX + endY * endY
            if (segLenSq < 1e-12) continue
            val t = ((riderX * endX + riderY * endY) / segLenSq).coerceIn(0.0, 1.0)
            val dx = riderX - t * endX
            val dy = riderY - t * endY
            val distSq = dx * dx + dy * dy
            val segLen = kotlin.math.sqrt(segLenSq)
            if (distSq < bestDistSq) {
                bestDistSq = distSq
                bestProgress = traversed + segLen * t
            }
            traversed += segLen
        }
        return ProgressProjection(
            progressM = bestProgress,
            distanceToRouteM = kotlin.math.sqrt(bestDistSq),
        )
    }

    private fun recordReroutingAttempt(now: Double): Double {
        reroutingAttemptTimestamps.removeAll { now - it >= REROUTING_BACKOFF_WINDOW_MS }
        reroutingAttemptTimestamps += now
        val count = reroutingAttemptTimestamps.size
        val delayMs = when {
            count >= REROUTING_ESCALATE_AT_ATTEMPTS -> REROUTING_BACKOFF_LONG_DELAY_MS
            count >= REROUTING_THROTTLE_AT_ATTEMPTS -> REROUTING_BACKOFF_DELAY_MS
            else -> 0.0
        }
        reroutingDelayedUntilMs = if (delayMs > 0.0) now + delayMs else null
        return delayMs
    }

    fun isWaitingToReroute(now: Double): Boolean {
        val until = reroutingDelayedUntilMs ?: return false
        return now < until
    }

    fun requestManualReroute() {
        reroutingDelayedUntilMs = null
    }

    private fun markAutoRerouteDispatched() {
        rerouteRequested = false
        offRouteDurationMs = 0.0
        reroutingDelayedUntilMs = null
    }

    /**
     * Split a polyline at a given distance-along-route. Returns the prefix up
     * to the split distance and the suffix from there to the end. Inserts a
     * synthesized vertex on the segment containing the split point so each
     * side stays a valid polyline. Mirrors `splitPolylineAtDistance` on web
     * and iOS.
     */
    fun splitPolylineAtDistance(
        polyline: List<CoordinatePoint>,
        distance: Double,
    ): PolylineSplit {
        if (polyline.size < 2 || distance <= 0.0) return PolylineSplit(emptyList(), polyline)
        val metersPerDegLat = 111_320.0
        var traversed = 0.0
        val completed = mutableListOf(polyline[0])
        for (i in 0 until polyline.size - 1) {
            val a = polyline[i]
            val b = polyline[i + 1]
            val meanLat = ((a.latitude + b.latitude) / 2.0) * Math.PI / 180.0
            val dN = (b.latitude - a.latitude) * metersPerDegLat
            val dE = (b.longitude - a.longitude) * kotlin.math.cos(meanLat) * metersPerDegLat
            val segLen = kotlin.math.sqrt(dN * dN + dE * dE)
            if (traversed + segLen >= distance) {
                val t = if (segLen <= 0.0) 0.0 else (distance - traversed) / segLen
                val split = CoordinatePoint(
                    latitude = a.latitude + (b.latitude - a.latitude) * t,
                    longitude = a.longitude + (b.longitude - a.longitude) * t,
                )
                completed += split
                val remaining = mutableListOf(split)
                remaining += polyline.subList(i + 1, polyline.size)
                return PolylineSplit(completed, remaining)
            }
            completed += b
            traversed += segLen
        }
        return PolylineSplit(polyline, listOf(polyline.last()))
    }

    data class PolylineSplit(
        val completed: List<CoordinatePoint>,
        val remaining: List<CoordinatePoint>,
    )

    private fun declareArrival() {
        arrivalNotice = "Arrived at destination"
        scheduleArrivalNoticeAutoDismiss()
        // Reuse the manual-stop teardown so persistence + UI stay consistent.
        // arrivalNotice survives because stopActiveNavigation() doesn't clear it.
        stopActiveNavigation()
    }

    /** Manual dismissal from the banner's close button. */
    fun dismissArrivalNotice() {
        cancelArrivalNoticeAutoDismiss()
        arrivalNotice = null
    }

    private fun scheduleArrivalNoticeAutoDismiss() {
        cancelArrivalNoticeAutoDismiss()
        arrivalNoticeAutoDismissJob = arrivalNoticeScope.launch {
            delay(arrivalNoticeAutoDismissMs)
            arrivalNotice = null
        }
    }

    private fun cancelArrivalNoticeAutoDismiss() {
        arrivalNoticeAutoDismissJob?.cancel()
        arrivalNoticeAutoDismissJob = null
    }

    companion object {
        private const val OFF_ROUTE_ENTER_DISTANCE_M = 35.0
        private const val OFF_ROUTE_EXIT_DISTANCE_M = 22.0
        private const val REROUTE_REQUEST_DELAY_MS = 2_000.0
        private const val REROUTING_BACKOFF_WINDOW_MS = 30_000.0
        private const val REROUTING_THROTTLE_AT_ATTEMPTS = 3
        private const val REROUTING_ESCALATE_AT_ATTEMPTS = 5
        private const val REROUTING_BACKOFF_DELAY_MS = 5_000.0
        private const val REROUTING_BACKOFF_LONG_DELAY_MS = 10_000.0
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
