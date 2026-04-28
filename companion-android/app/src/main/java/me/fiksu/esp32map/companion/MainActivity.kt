package me.fiksu.esp32map.companion

import android.content.Intent
import android.content.pm.PackageManager
import android.os.Bundle
import androidx.activity.ComponentActivity
import androidx.activity.compose.BackHandler
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.compose.setContent
import androidx.activity.enableEdgeToEdge
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.ExperimentalFoundationApi
import androidx.compose.foundation.combinedClickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ColumnScope
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.material3.Button
import androidx.compose.material3.ButtonDefaults
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.FilterChip
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.material3.TextField
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalFocusManager
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.LocalUriHandler
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.core.content.ContextCompat
import androidx.activity.viewModels
import com.google.android.gms.maps.CameraUpdateFactory
import com.google.android.gms.maps.model.CameraPosition
import com.google.android.gms.maps.model.LatLng
import com.google.android.gms.maps.model.LatLngBounds
import com.google.maps.android.compose.GoogleMap
import com.google.maps.android.compose.MapProperties
import com.google.maps.android.compose.MapUiSettings
import com.google.maps.android.compose.Marker
import com.google.maps.android.compose.MarkerState
import com.google.maps.android.compose.Polyline
import com.google.maps.android.compose.rememberCameraPositionState
import kotlinx.coroutines.launch
import kotlin.math.PI
import kotlin.math.atan2
import kotlin.math.cos
import me.fiksu.esp32map.companion.app.CompanionAppState
import me.fiksu.esp32map.companion.app.markSharedIntentConsumed
import me.fiksu.esp32map.companion.domain.CoordinatePoint
import me.fiksu.esp32map.companion.domain.HomeCompassMode
import me.fiksu.esp32map.companion.domain.HomeMode
import me.fiksu.esp32map.companion.domain.RouteHistoryItem
import me.fiksu.esp32map.companion.domain.RouteSourceMode
import me.fiksu.esp32map.companion.domain.SpeedUnit
import me.fiksu.esp32map.companion.domain.RouteStartBehavior
import me.fiksu.esp32map.companion.domain.RouteSuggestionMode
import me.fiksu.esp32map.companion.feature.home.HomeStateHolder
import me.fiksu.esp32map.companion.integration.AndroidPlaceSearchService

class MainActivity : ComponentActivity() {
    private val appState: CompanionAppState by viewModels()

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        enableEdgeToEdge()
        setContent {
            MaterialTheme {
                CompanionApp(appState = appState)
            }
        }
        processSharedIntent(intent)
        if (appState.locationService.hasLocationPermission()) {
            appState.startLocationUpdates()
        }
    }

    override fun onStop() {
        super.onStop()
        appState.stopLocationUpdates()
    }

    override fun onNewIntent(intent: Intent) {
        super.onNewIntent(intent)
        setIntent(intent)
        processSharedIntent(intent)
    }

    private fun resolveSourceApplication(intent: Intent): String? {
        return intent.getStringExtra(Intent.EXTRA_REFERRER_NAME)
            ?: referrer?.host
            ?: referrer?.toString()
    }

    private fun processSharedIntent(intent: Intent?) {
        intent ?: return
        if (appState.handleSharedIntent(intent, resolveSourceApplication(intent))) {
            markSharedIntentConsumed(intent)
            setIntent(intent)
        }
    }
}

private enum class SettingsDestination {
    ROOT,
    ROUTES,
    DEVICE,
    ROUTE_PLANNER,
    IMPORT_DIAGNOSTICS,
}

@Composable
private fun CompanionApp(appState: CompanionAppState) {
    val context = LocalContext.current
    val homeState = remember(appState) { HomeStateHolder(appState, AndroidPlaceSearchService(context)) }
    var showingSettings by rememberSaveable { mutableStateOf(false) }
    var settingsDestination by rememberSaveable { mutableStateOf(SettingsDestination.ROOT) }
    var selectedRouteId by rememberSaveable { mutableStateOf<String?>(null) }

    LaunchedEffect(appState.shareImportEventId) {
        if (appState.shareImportEventId != 0L) {
            showingSettings = false
            settingsDestination = SettingsDestination.ROOT
            selectedRouteId = null
            homeState.syncQueryFromPreview()
        }
    }

    Box(Modifier.fillMaxSize()) {
        if (showingSettings) {
            BackHandler {
                if (settingsDestination == SettingsDestination.ROOT) {
                    showingSettings = false
                } else {
                    settingsDestination = SettingsDestination.ROOT
                }
            }
        }
        if (selectedRouteId != null) {
            BackHandler { selectedRouteId = null }
        }

        CompanionHomeScreen(
            appState = appState,
            homeState = homeState,
            onOpenSettings = { showingSettings = true },
        )

        if (showingSettings) {
            FullScreenOverlay {
                when (settingsDestination) {
                    SettingsDestination.ROOT -> SettingsRootScreen(
                        onDismiss = {
                            showingSettings = false
                            settingsDestination = SettingsDestination.ROOT
                        },
                        onRoutes = { settingsDestination = SettingsDestination.ROUTES },
                        onDevice = { settingsDestination = SettingsDestination.DEVICE },
                        onRoutePlanner = { settingsDestination = SettingsDestination.ROUTE_PLANNER },
                        onImportDiagnostics = { settingsDestination = SettingsDestination.IMPORT_DIAGNOSTICS },
                    )
                    SettingsDestination.ROUTES -> RoutesSettingsScreen(
                        appState = appState,
                        onBack = { settingsDestination = SettingsDestination.ROOT },
                        onOpenRoute = { selectedRouteId = it },
                    )
                    SettingsDestination.DEVICE -> DeviceSettingsScreen(appState = appState, onBack = { settingsDestination = SettingsDestination.ROOT })
                    SettingsDestination.ROUTE_PLANNER -> RoutePlannerSettingsScreen(appState = appState, onBack = { settingsDestination = SettingsDestination.ROOT })
                    SettingsDestination.IMPORT_DIAGNOSTICS -> ImportDiagnosticsScreen(appState = appState, onBack = { settingsDestination = SettingsDestination.ROOT })
                }
            }
        }

        selectedRouteId?.let { routeId ->
            val item = appState.routeHistoryItems.firstOrNull { it.id == routeId }
            if (item != null) {
                FullScreenOverlay {
                    RouteDetailScreen(
                        item = item,
                        onBack = { selectedRouteId = null },
                        onOpen = { appState.applyRouteHistoryPreview(item) { selectedRouteId = null } },
                        onStart = {
                            appState.applyRouteHistoryPreview(item) {
                                homeState.startSelectedRoute()
                                selectedRouteId = null
                            }
                        },
                        onDismiss = {
                            appState.dismissRouteHistoryItem(item.id)
                            selectedRouteId = null
                        },
                    )
                }
            }
        }
    }
}

@Composable
private fun CompanionHomeScreen(
    appState: CompanionAppState,
    homeState: HomeStateHolder,
    onOpenSettings: () -> Unit,
) {
    val scope = rememberCoroutineScope()
    val cameraPositionState = rememberCameraPositionState {
        position = CameraPosition.fromLatLngZoom(LatLng(60.1699, 24.9384), 13f)
    }

    LaunchedEffect(
        homeState.displayedRouteCoordinates,
        homeState.homeMode,
        homeState.compassMode,
        homeState.mapRecenterRequestTick,
        homeState.mapFollowRiderTick,
        homeState.travelHeadingDegrees,
    ) {
        val coordinates = homeState.displayedRouteCoordinates
        // Spec lines 108-118 ("when moving with or without a route"): if
        // the rider is moving, enter riding-mode camera regardless of
        // whether a route is loaded — bottom-quarter anchor + GPS-trail
        // heading. Falls through to fit/no-op when stationary.
        val trailHeading = homeState.travelHeadingDegrees
        val ridingZoom = appState.settings.ridingZoom ?: 16f
        if (homeState.homeMode == HomeMode.PLANNING && trailHeading != null) {
            orientCameraForTravel(
                cameraPositionState,
                coordinates,
                rider = appState.riderLocation,
                bearingDegrees = trailHeading,
                ridingZoom = ridingZoom,
            )
            return@LaunchedEffect
        }
        if (coordinates.isEmpty()) return@LaunchedEffect
        when (homeState.homeMode) {
            HomeMode.PLANNING, HomeMode.DEVICE_OVERVIEW, HomeMode.SENDING_TO_DEVICE -> fitCameraToRoute(cameraPositionState, coordinates)
            HomeMode.PHONE_GUIDANCE -> when (homeState.compassMode) {
                HomeCompassMode.AUTO_FOLLOW -> orientCameraForTravel(
                    cameraPositionState,
                    coordinates,
                    rider = appState.riderLocation,
                    bearingDegrees = homeState.cameraHeadingDegrees(appState.riderLocation),
                    ridingZoom = ridingZoom,
                )
                HomeCompassMode.NORTH_PREVIEW, HomeCompassMode.NORTH_LOCKED -> {
                    // Spec #7: route-overview during routing should fit only
                    // the remaining geometry, not the completed prefix —
                    // otherwise late in a ride the camera zooms out far past
                    // anything useful.
                    val remaining = homeState.routeOverviewGeometry
                    val toFit = if (remaining.size >= 2) remaining else coordinates
                    fitCameraToRoute(cameraPositionState, toFit)
                }
            }
        }
    }

    // Spec lines 84 + 110: every GPS fix feeds the heading trail (so the
    // camera can rotate to riding direction in any mode) AND notifies the
    // viewmodel which bumps the follow-rider tick when in routing or moving.
    LaunchedEffect(appState.locationState.currentLocation) {
        appState.locationState.currentLocation?.let { fix ->
            homeState.ingestRiderLocationFix(fix, System.currentTimeMillis())
        }
        homeState.notifyRiderLocationUpdated()
    }

    // Spec line 104: any non-programmatic camera change during routing
    // schedules an auto-recenter after the pinned inactivity timeout.
    LaunchedEffect(cameraPositionState.isMoving) {
        if (cameraPositionState.isMoving && homeState.homeMode == HomeMode.PHONE_GUIDANCE) {
            homeState.noteUserMapInteraction(scope)
        }
    }

    Box(Modifier.fillMaxSize()) {
        GoogleMap(
            modifier = Modifier.fillMaxSize(),
            cameraPositionState = cameraPositionState,
            uiSettings = MapUiSettings(
                compassEnabled = true,
                myLocationButtonEnabled = appState.locationService.hasLocationPermission(),
            ),
            properties = MapProperties(isMyLocationEnabled = appState.locationService.hasLocationPermission()),
            onMapLongClick = { latLng ->
                if (homeState.homeMode == HomeMode.PLANNING) {
                    homeState.setDestinationFromMap(CoordinatePoint(latLng.latitude, latLng.longitude), scope)
                }
            },
            onMapClick = {
                // A regular tap on the map (anywhere outside the search overlay) collapses
                // the dropdown and drops keyboard focus. Mirrors the web outside-click dismiss.
                if (homeState.shouldShowSearchPanel) homeState.closeSearch()
            },
        ) {
            if (homeState.homeMode == HomeMode.PLANNING) {
                homeState.previewAlternatives.forEach { alternative ->
                    Polyline(
                        points = alternative.normalizedPackage.geometry.map { LatLng(it.latitude, it.longitude) },
                        color = if (alternative.id == appState.preview.selectedAlternativeId) Color(0xFF2D6CDF) else Color(0x6626A69A),
                        width = if (alternative.id == appState.preview.selectedAlternativeId) 10f else 7f,
                    )
                }
            } else {
                homeState.guidanceRoute?.let { active ->
                    Polyline(
                        points = active.geometry.map { LatLng(it.latitude, it.longitude) },
                        color = if (homeState.homeMode == HomeMode.DEVICE_OVERVIEW) Color(0xFF2D6CDF) else Color(0xFF2E7D32),
                        width = 12f,
                    )
                }
            }
            homeState.destinationCoordinate?.let {
                Marker(state = MarkerState(LatLng(it.latitude, it.longitude)), title = "Destination")
            }
        }

        Column(modifier = Modifier.fillMaxSize().padding(16.dp)) {
            when (homeState.homeMode) {
                HomeMode.PLANNING -> PlanningTopArea(appState, homeState, scope, onOpenSettings)
                HomeMode.PHONE_GUIDANCE -> PhoneGuidanceTopArea(appState, homeState, scope)
                HomeMode.SENDING_TO_DEVICE, HomeMode.DEVICE_OVERVIEW -> DeviceOverviewTopArea(homeState, onOpenSettings)
            }

            Spacer(Modifier.weight(1f))

            when (homeState.homeMode) {
                HomeMode.PLANNING -> {
                    val arrival = homeState.arrivalNotice
                    if (arrival != null) {
                        ArrivalCard(arrival)
                    } else if (homeState.previewAlternatives.isNotEmpty()) {
                        RouteSuggestionsCard(appState, homeState)
                    }
                }
                HomeMode.PHONE_GUIDANCE -> ActiveGuidanceCard(homeState)
                HomeMode.SENDING_TO_DEVICE, HomeMode.DEVICE_OVERVIEW -> DeviceOverviewCard(appState, homeState)
            }
            SpeedBadge(appState = appState, homeState = homeState)
        }

        // Spec line 10: zoom +/- buttons under the top bar on the right.
        // Riding-mode taps persist (`appState.settings.ridingZoom`) so the
        // rider's preferred navigation zoom carries over between rides;
        // overview/planning taps animate the camera now but aren't
        // persisted.
        Column(
            modifier = Modifier
                .align(Alignment.TopEnd)
                .padding(top = 96.dp, end = 16.dp),
            verticalArrangement = Arrangement.spacedBy(8.dp),
        ) {
            ZoomButton(label = "Zoom in", glyph = "+") {
                applyZoomDelta(scope, cameraPositionState, appState, homeState, +1f)
            }
            ZoomButton(label = "Zoom out", glyph = "−") {
                applyZoomDelta(scope, cameraPositionState, appState, homeState, -1f)
            }
        }
    }
}

@Composable
private fun ZoomButton(label: String, glyph: String, onClick: () -> Unit) {
    IconButton(
        onClick = onClick,
        modifier = Modifier
            .size(44.dp)
            .background(
                color = Color(0xCC1F2937),
                shape = RoundedCornerShape(14.dp),
            )
            .semantics { contentDescription = label },
    ) {
        Text(
            text = glyph,
            color = Color.White,
            fontSize = 22.sp,
            fontWeight = FontWeight.SemiBold,
        )
    }
}

private fun applyZoomDelta(
    scope: kotlinx.coroutines.CoroutineScope,
    cameraPositionState: com.google.maps.android.compose.CameraPositionState,
    appState: CompanionAppState,
    homeState: HomeStateHolder,
    delta: Float,
) {
    val current = cameraPositionState.position.zoom
    val next = (current + delta).coerceIn(2f, 21f)
    if (homeState.homeMode == HomeMode.PHONE_GUIDANCE) {
        // Persist so future rides start at the rider's preferred scale.
        appState.settings = appState.settings.copy(ridingZoom = next)
        appState.persistSettings()
    }
    scope.launch {
        cameraPositionState.animate(CameraUpdateFactory.zoomTo(next))
    }
}

@Composable
private fun SpeedBadge(appState: CompanionAppState, homeState: HomeStateHolder) {
    // Spec: render speed whenever the rider is moving (with or without an
    // active route). The "moving" signal is the heading-trail's
    // travelHeadingDegrees — same threshold the camera uses to enter
    // routing-anchor mode — so the badge appears/disappears in lock-step
    // with the bottom-quarter anchor.
    val moving = homeState.travelHeadingDegrees != null
    val inGuidance = homeState.homeMode == HomeMode.PHONE_GUIDANCE
    if (!moving && !inGuidance) return
    val unit = appState.settings.speedUnit
    val mps = appState.locationState.currentSpeedMps
    val factor = if (unit == SpeedUnit.MPH) 2.2369363 else 3.6
    val label = if (mps == null || !mps.isFinite()) {
        "— ${unit.label}"
    } else {
        "${kotlin.math.round(mps * factor).toInt()} ${unit.label}"
    }
    Row(modifier = Modifier.fillMaxWidth().padding(top = 8.dp)) {
        Surface(shape = MaterialTheme.shapes.medium, tonalElevation = 4.dp) {
            Text(label, modifier = Modifier.padding(horizontal = 12.dp, vertical = 8.dp), style = MaterialTheme.typography.titleSmall)
        }
    }
}

@Composable
private fun PlanningTopArea(
    appState: CompanionAppState,
    homeState: HomeStateHolder,
    scope: kotlinx.coroutines.CoroutineScope,
    onOpenSettings: () -> Unit,
) {
    val focusManager = LocalFocusManager.current
    BackHandler(enabled = homeState.shouldShowSearchPanel) {
        focusManager.clearFocus(force = true)
        homeState.closeSearch()
    }
    Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
        LocationPermissionSection(appState)
        Row(verticalAlignment = Alignment.CenterVertically) {
            TextField(
                value = homeState.query,
                onValueChange = {
                    homeState.openSearch()
                    homeState.updateQuery(it, scope)
                },
                label = { Text("Where to?") },
                singleLine = true,
                keyboardOptions = androidx.compose.foundation.text.KeyboardOptions(
                    imeAction = androidx.compose.ui.text.input.ImeAction.Done,
                ),
                keyboardActions = androidx.compose.foundation.text.KeyboardActions(
                    onDone = {
                        focusManager.clearFocus(force = true)
                        homeState.closeSearch()
                    },
                ),
                modifier = Modifier.weight(1f),
            )
            Spacer(Modifier.width(12.dp))
            IconButton(onClick = onOpenSettings) {
                Text("Set")
            }
        }

        if (homeState.shouldShowSearchPanel) {
            Surface(
                modifier = Modifier.fillMaxWidth().padding(top = 4.dp),
                tonalElevation = 4.dp,
                shape = MaterialTheme.shapes.large,
            ) {
                if (homeState.isResolvingUrl) {
                    Row(
                        modifier = Modifier.fillMaxWidth().padding(14.dp),
                        verticalAlignment = Alignment.CenterVertically,
                        horizontalArrangement = Arrangement.spacedBy(12.dp),
                    ) {
                        CircularProgressIndicator(modifier = Modifier.size(20.dp), strokeWidth = 2.dp)
                        Column(Modifier.weight(1f)) {
                            Text("Resolving link…", fontWeight = FontWeight.SemiBold)
                            Text(
                                "Following the URL to a destination. This can take a couple of seconds.",
                                style = MaterialTheme.typography.bodySmall,
                                color = MaterialTheme.colorScheme.onSurfaceVariant,
                            )
                        }
                    }
                } else if (homeState.urlResolveError != null) {
                    Column(
                        modifier = Modifier.fillMaxWidth().padding(14.dp),
                        verticalArrangement = Arrangement.spacedBy(4.dp),
                    ) {
                        Text("Couldn't open that link", fontWeight = FontWeight.SemiBold)
                        Text(
                            homeState.urlResolveError ?: "",
                            style = MaterialTheme.typography.bodySmall,
                            color = MaterialTheme.colorScheme.onSurfaceVariant,
                        )
                    }
                } else {
                    LazyColumn(modifier = Modifier.height(320.dp), contentPadding = PaddingValues(vertical = 8.dp)) {
                        if (homeState.query.isBlank()) {
                            items(homeState.recentItems, key = { it.id }) { item ->
                                RouteHistoryRow(item = item, onClick = {
                                    focusManager.clearFocus(force = true)
                                    homeState.selectRecent(item, scope)
                                })
                                homeState.loadMoreRecentsIfNeeded(item)
                            }
                        } else {
                            items(homeState.visibleSuggestions, key = { it.id }) { suggestion ->
                                SearchSuggestionRow(
                                    title = suggestion.title,
                                    subtitle = suggestion.subtitle,
                                    onClick = {
                                        focusManager.clearFocus(force = true)
                                        homeState.selectSuggestion(suggestion, scope)
                                    },
                                )
                                homeState.loadMoreSuggestionsIfNeeded(suggestion)
                            }
                        }
                    }
                }
            }
        }
    }
}

@OptIn(ExperimentalFoundationApi::class)
@Composable
private fun PhoneGuidanceTopArea(
    appState: CompanionAppState,
    homeState: HomeStateHolder,
    scope: kotlinx.coroutines.CoroutineScope,
) {
    // Top card: next-turn line as the headline, destination + remaining
    // bundled as the subtitle (single source of truth — the bottom just
    // floats a stop button). See HomeStateHolder.guidanceSubtitleLine.
    val headline = homeState.nextInstructionLine ?: homeState.activeNavigationTitle
    Surface(shape = MaterialTheme.shapes.large, tonalElevation = 4.dp) {
        Row(
            modifier = Modifier.fillMaxWidth().padding(14.dp),
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(12.dp),
        ) {
            Column(modifier = Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(4.dp)) {
                Text(headline, fontWeight = FontWeight.SemiBold)
                Text(homeState.guidanceSubtitleLine, style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
            }
            Box(
                modifier = Modifier
                    .size(48.dp)
                    .background(MaterialTheme.colorScheme.surfaceVariant, shape = MaterialTheme.shapes.medium)
                    .combinedClickable(
                        onClick = { homeState.handleCompassTap(scope) },
                        onDoubleClick = { homeState.handleCompassDoubleTap() },
                    ),
                contentAlignment = Alignment.Center,
            ) {
                if (appState.isWaitingForFirstLocationFix) {
                    CircularProgressIndicator(modifier = Modifier.size(20.dp), strokeWidth = 2.dp)
                } else {
                    Text(
                        when (homeState.compassMode) {
                            HomeCompassMode.AUTO_FOLLOW -> "Auto"
                            HomeCompassMode.NORTH_PREVIEW -> "N"
                            HomeCompassMode.NORTH_LOCKED -> "Lock"
                        },
                        style = MaterialTheme.typography.labelMedium,
                    )
                }
            }
        }
    }
}

@Composable
private fun DeviceOverviewTopArea(homeState: HomeStateHolder, onOpenSettings: () -> Unit) {
    Surface(shape = MaterialTheme.shapes.large, tonalElevation = 4.dp) {
        Row(
            modifier = Modifier.fillMaxWidth().padding(14.dp),
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(12.dp),
        ) {
            Column(modifier = Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(4.dp)) {
                Text(homeState.activeNavigationTitle, fontWeight = FontWeight.SemiBold)
                Text(homeState.activeNavigationSubtitle, style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
            }
            Button(onClick = onOpenSettings) { Text("Settings") }
        }
    }
}

@Composable
private fun SearchSuggestionRow(title: String, subtitle: String, onClick: () -> Unit) {
    Column(
        modifier = Modifier.fillMaxWidth().clickable(onClick = onClick).padding(horizontal = 16.dp, vertical = 12.dp),
        verticalArrangement = Arrangement.spacedBy(4.dp),
    ) {
        Text(title, fontWeight = FontWeight.SemiBold)
        if (subtitle.isNotBlank()) {
            Text(subtitle, style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
        }
    }
}

@Composable
private fun RouteHistoryRow(item: RouteHistoryItem, onClick: () -> Unit) {
    Column(
        modifier = Modifier.fillMaxWidth().clickable(onClick = onClick).padding(horizontal = 16.dp, vertical = 12.dp),
        verticalArrangement = Arrangement.spacedBy(4.dp),
    ) {
        Text(item.title, fontWeight = FontWeight.SemiBold)
        Text(item.subtitle, style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
        Text(item.sourceLabel, style = MaterialTheme.typography.labelSmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
    }
}

@Composable
private fun RouteSuggestionsCard(appState: CompanionAppState, homeState: HomeStateHolder) {
    Column(
        modifier = Modifier.fillMaxWidth().background(MaterialTheme.colorScheme.surface.copy(alpha = 0.94f), shape = MaterialTheme.shapes.extraLarge).padding(16.dp),
        verticalArrangement = Arrangement.spacedBy(8.dp),
    ) {
        Row(verticalAlignment = Alignment.CenterVertically) {
            Text(homeState.routeSuggestionsTitle, style = MaterialTheme.typography.titleMedium, modifier = Modifier.weight(1f))
            Button(onClick = { homeState.clearPreview() }) { Text("Close") }
        }
        appState.preview.planningNotice?.takeIf { it.isNotBlank() }?.let {
            Text(it, style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
        }
        if (homeState.shouldShowSourceControl) {
            Row(horizontalArrangement = Arrangement.spacedBy(8.dp), modifier = Modifier.fillMaxWidth()) {
                appState.sourceModeOptions.forEach { mode ->
                    FilterChip(
                        selected = homeState.sourceMode == mode,
                        onClick = { homeState.setSourceMode(mode) },
                        label = { Text(mode.displayName) },
                    )
                }
            }
        }
        homeState.previewAlternatives.forEach { alternative ->
            val selected = alternative.id == appState.preview.selectedAlternativeId
            Row(
                modifier = Modifier.fillMaxWidth().clickable { homeState.selectAlternative(alternative.id) }.background(if (selected) MaterialTheme.colorScheme.primary.copy(alpha = 0.12f) else Color.Transparent, shape = MaterialTheme.shapes.medium).padding(12.dp),
                verticalAlignment = Alignment.CenterVertically,
            ) {
                Column(Modifier.weight(1f)) {
                    Text(alternative.title, fontWeight = FontWeight.SemiBold)
                    Text(alternative.subtitle, style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
                    Text(alternative.normalizedPackage.summaryLine, style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
                }
                if (selected) {
                    Text("Selected", color = MaterialTheme.colorScheme.primary, style = MaterialTheme.typography.labelMedium)
                }
            }
        }
        Button(onClick = { homeState.startSelectedRoute() }, modifier = Modifier.fillMaxWidth(), enabled = homeState.homeMode != HomeMode.SENDING_TO_DEVICE) {
            if (homeState.homeMode == HomeMode.SENDING_TO_DEVICE) {
                Row(horizontalArrangement = Arrangement.spacedBy(8.dp), verticalAlignment = Alignment.CenterVertically) {
                    CircularProgressIndicator(modifier = Modifier.size(18.dp), strokeWidth = 2.dp)
                    Text(homeState.startButtonTitle)
                }
            } else {
                Text(homeState.startButtonTitle)
            }
        }
    }
}

@Composable
private fun ActiveGuidanceCard(homeState: HomeStateHolder) {
    // Destination + remaining + ETA all live in the top card now (see
    // `guidanceSubtitleLine`). The bottom slot is intentionally minimal:
    // a floating Stop button so the map stays maximally visible.
    if (homeState.guidanceRoute == null) return
    Row(
        modifier = Modifier.fillMaxWidth().padding(horizontal = 16.dp, vertical = 8.dp),
        horizontalArrangement = Arrangement.End,
    ) {
        Button(
            onClick = { homeState.stopActiveNavigation() },
            colors = ButtonDefaults.buttonColors(containerColor = MaterialTheme.colorScheme.error),
        ) {
            Text("Stop")
        }
    }
}

@Composable
private fun ArrivalCard(message: String) {
    Column(
        modifier = Modifier.fillMaxWidth().background(MaterialTheme.colorScheme.surface.copy(alpha = 0.94f), shape = MaterialTheme.shapes.extraLarge).padding(16.dp),
        verticalArrangement = Arrangement.spacedBy(4.dp),
    ) {
        Text(message, style = MaterialTheme.typography.titleMedium)
        Text(
            "Routing finished. Tap a destination to plan again.",
            style = MaterialTheme.typography.bodySmall,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
        )
    }
}

@Composable
private fun DeviceOverviewCard(appState: CompanionAppState, homeState: HomeStateHolder) {
    val route = homeState.guidanceRoute ?: return
    Column(
        modifier = Modifier.fillMaxWidth().background(MaterialTheme.colorScheme.surface.copy(alpha = 0.94f), shape = MaterialTheme.shapes.extraLarge).padding(16.dp),
        verticalArrangement = Arrangement.spacedBy(8.dp),
    ) {
        Text(route.summary.destinationLabel ?: "Route active on device", style = MaterialTheme.typography.titleMedium)
        Text(appState.syncSession.lastSyncResult, style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
        appState.syncSession.transferProgress?.let { progress ->
            Text("Sending ${progress.percentComplete}%", style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
        }
        Button(onClick = { homeState.stopActiveNavigation() }, modifier = Modifier.fillMaxWidth()) {
            Text("Stop")
        }
    }
}

@Composable
private fun SettingsRootScreen(
    onDismiss: () -> Unit,
    onRoutes: () -> Unit,
    onDevice: () -> Unit,
    onRoutePlanner: () -> Unit,
    onImportDiagnostics: () -> Unit,
) {
    ScreenColumn(PaddingValues(0.dp)) {
        Row(verticalAlignment = Alignment.CenterVertically) {
            Text("Settings", style = MaterialTheme.typography.headlineSmall, modifier = Modifier.weight(1f))
            Button(onClick = onDismiss) { Text("Done") }
        }
        Button(onClick = onRoutes, modifier = Modifier.fillMaxWidth()) { Text("Routes") }
        Button(onClick = onDevice, modifier = Modifier.fillMaxWidth()) { Text("Device") }
        Button(onClick = onRoutePlanner, modifier = Modifier.fillMaxWidth()) { Text("Route Planner") }
        Button(onClick = onImportDiagnostics, modifier = Modifier.fillMaxWidth()) { Text("Import Diagnostics") }
    }
}

@Composable
private fun ImportDiagnosticsScreen(appState: CompanionAppState, onBack: () -> Unit) {
    val context = LocalContext.current
    val clipboard = context.getSystemService(android.content.ClipboardManager::class.java)
    ScreenColumn(PaddingValues(0.dp)) {
        BackHeader(title = "Import Diagnostics", onBack = onBack)
        if (appState.importDiagnosticsEntries.isEmpty()) {
            Text("No unsupported shared items yet.", color = MaterialTheme.colorScheme.onSurfaceVariant)
        } else {
            appState.importDiagnosticsEntries.forEach { entry ->
                Column(
                    modifier = Modifier
                        .fillMaxWidth()
                        .background(MaterialTheme.colorScheme.surfaceVariant, shape = MaterialTheme.shapes.large)
                        .padding(12.dp),
                    verticalArrangement = Arrangement.spacedBy(6.dp),
                ) {
                    Text(entry.title, fontWeight = FontWeight.SemiBold)
                    Text(entry.subtitle, style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
                    Text(entry.envelope.classification.name, style = MaterialTheme.typography.labelSmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
                    Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                        Button(onClick = {
                            clipboard?.setPrimaryClip(android.content.ClipData.newPlainText("import-diagnostics", entry.envelope.toString()))
                        }) { Text("Copy debug info") }
                        Button(onClick = { shareDebugPackage(context, entry.envelope.toString()) }) { Text("Share debug package") }
                    }
                    Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                        Button(onClick = { appState.retrySharedImport(entry) }) { Text("Retry import") }
                        Button(onClick = { appState.dismissImportDiagnosticsEntry(entry.id) }) { Text("Dismiss") }
                    }
                }
            }
        }
    }
}
@Composable
private fun RoutesSettingsScreen(appState: CompanionAppState, onBack: () -> Unit, onOpenRoute: (String) -> Unit) {
    val context = LocalContext.current
    val gpxLauncher = rememberLauncherForActivityResult(ActivityResultContracts.OpenDocument()) { uri ->
        if (uri != null) appState.importGpxUri(context, uri)
    }
    ScreenColumn(PaddingValues(0.dp)) {
        BackHeader(title = "Routes", onBack = onBack)
        Button(onClick = { gpxLauncher.launch(arrayOf("application/gpx+xml", "application/xml", "text/xml", "*/*")) }, modifier = Modifier.fillMaxWidth()) {
            Text("Import GPX")
        }
        if (appState.routeHistoryItems.isEmpty()) {
            Text("No recent routes or destinations yet.", color = MaterialTheme.colorScheme.onSurfaceVariant)
        } else {
            appState.routeHistoryItems.forEach { item ->
                RouteHistoryRow(item = item) { onOpenRoute(item.id) }
            }
        }
    }
}

@Composable
private fun DeviceSettingsScreen(appState: CompanionAppState, onBack: () -> Unit) {
    ScreenColumn(PaddingValues(0.dp)) {
        BackHeader(title = "Device", onBack = onBack)
        BluetoothPermissionSection()
        LocationPermissionSection(appState)
        Text("Connection: ${appState.syncSession.connectionState}")
        Text("Route sync: ${appState.syncSession.routeSyncState}")
        Text("Pending route: ${appState.syncSession.pendingRouteIdentifier ?: "None"}")
        Text("Active route: ${appState.syncSession.activeRouteIdentifier ?: "None"}")
        Text("Last sync: ${appState.syncSession.lastSyncResult}")
        Button(onClick = appState::connectToDevice) { Text("Reconnect") }
        Button(onClick = { appState.sendSelectedRoute() }) { Text("Send selected route") }
        Button(onClick = appState::resumePendingTransfer) { Text("Resume pending transfer") }
        Button(onClick = appState::armRetryableInterruptionOnNextTransfer) { Text("Arm retryable interruption") }
        Button(onClick = appState::armWriteFailureOnNextTransfer) { Text("Arm write failure") }
        Button(onClick = appState::armDisconnectAfterNextChunkWrite) { Text("Arm disconnect after chunk") }
        Button(onClick = appState::armDropNextInboundStatus) { Text("Arm drop next inbound status") }
        Button(onClick = { appState.clearActiveRoute() }) { Text("Clear active route") }
    }
}

@Composable
private fun RoutePlannerSettingsScreen(appState: CompanionAppState, onBack: () -> Unit) {
    ScreenColumn(PaddingValues(0.dp)) {
        BackHeader(title = "Route Planner", onBack = onBack)
        if (appState.sourceModeOptions.size > 1) {
            Text("Default route source")
            appState.sourceModeOptions.forEach { mode ->
                FilterChip(
                    selected = appState.routePlannerPreferences.defaultSourceMode == mode,
                    onClick = {
                        appState.saveRoutePlannerPreferences(appState.routePlannerPreferences.copy(defaultSourceMode = mode))
                        appState.currentSourceMode = mode
                    },
                    label = { Text(mode.displayName) },
                )
            }
        }
        Text("Suggestion mode")
        RouteSuggestionMode.entries.forEach { mode ->
            FilterChip(
                selected = appState.routePlannerPreferences.suggestionMode == mode,
                onClick = { appState.saveRoutePlannerPreferences(appState.routePlannerPreferences.copy(suggestionMode = mode)) },
                label = { Text(mode.displayName) },
            )
        }
        Text("Start behavior")
        RouteStartBehavior.entries.forEach { behavior ->
            FilterChip(
                selected = appState.routePlannerPreferences.startBehavior == behavior,
                onClick = { appState.saveRoutePlannerPreferences(appState.routePlannerPreferences.copy(startBehavior = behavior)) },
                label = { Text(behavior.displayName) },
            )
        }

        Spacer(Modifier.height(16.dp))
        Text("Riding", fontWeight = FontWeight.SemiBold)
        Text(
            "Cycling speed overrides HSL ETAs (Digitransit defaults to a slow bike speed). Speed unit is how the live-speed badge is shown on the map.",
            style = MaterialTheme.typography.bodySmall,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
        )
        Row(verticalAlignment = Alignment.CenterVertically) {
            Text("Cycling speed", modifier = Modifier.weight(1f))
            TextField(
                value = appState.settings.cyclingSpeedKph.toInt().toString(),
                onValueChange = { raw ->
                    val parsed = raw.toIntOrNull()
                    if (parsed != null && parsed > 0) {
                        appState.settings = appState.settings.copy(cyclingSpeedKph = parsed.toDouble())
                        appState.persistSettings()
                    }
                },
                singleLine = true,
                keyboardOptions = androidx.compose.foundation.text.KeyboardOptions(
                    keyboardType = androidx.compose.ui.text.input.KeyboardType.Number,
                ),
                label = { Text("kph") },
                modifier = Modifier.width(120.dp),
            )
        }
        Text("Speed unit")
        SpeedUnit.entries.forEach { unit ->
            FilterChip(
                selected = appState.settings.speedUnit == unit,
                onClick = {
                    appState.settings = appState.settings.copy(speedUnit = unit)
                    appState.persistSettings()
                },
                label = { Text(unit.label) },
            )
        }

        Spacer(Modifier.height(16.dp))
        Text("HSL Digitransit", fontWeight = FontWeight.SemiBold)
        Text(
            "HSL is the Helsinki Region Transport authority. Their Digitransit API provides high-quality bike routing across the Helsinki metro area. The key is free — sign in at the portal, register an app, and copy the subscription key into the field below. Outside the Helsinki area, leave HSL off and the planner uses OSM routing globally.",
            style = MaterialTheme.typography.bodySmall,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
        )
        val uriHandler = LocalUriHandler.current
        Text(
            "Open the Digitransit portal",
            style = MaterialTheme.typography.bodySmall,
            color = MaterialTheme.colorScheme.primary,
            modifier = Modifier.clickable { uriHandler.openUri("https://portal-api.digitransit.fi/") },
        )
        Spacer(Modifier.height(8.dp))
        TextField(
            value = appState.settings.hslSubscriptionKey,
            onValueChange = {
                appState.settings = appState.settings.copy(hslSubscriptionKey = it)
                appState.persistSettings()
            },
            label = { Text("Digitransit subscription key") },
            modifier = Modifier.fillMaxWidth(),
        )
    }
}

@Composable
private fun RouteDetailScreen(
    item: RouteHistoryItem,
    onBack: () -> Unit,
    onOpen: () -> Unit,
    onStart: () -> Unit,
    onDismiss: () -> Unit,
) {
    ScreenColumn(PaddingValues(0.dp)) {
        BackHeader(title = item.title, onBack = onBack)
        Text(item.sourceLabel, color = MaterialTheme.colorScheme.onSurfaceVariant)
        Text(item.subtitle, color = MaterialTheme.colorScheme.onSurfaceVariant)
        Spacer(Modifier.height(8.dp))
        Button(onClick = onOpen, modifier = Modifier.fillMaxWidth()) { Text("Open") }
        Button(onClick = onStart, modifier = Modifier.fillMaxWidth()) { Text("Start") }
        Button(onClick = onDismiss, modifier = Modifier.fillMaxWidth()) { Text("Dismiss") }
    }
}

@Composable
private fun FullScreenOverlay(content: @Composable () -> Unit) {
    Surface(modifier = Modifier.fillMaxSize(), color = MaterialTheme.colorScheme.background) {
        Box(Modifier.fillMaxSize()) {
            content()
        }
    }
}

@Composable
private fun ScreenColumn(padding: PaddingValues, content: @Composable ColumnScope.() -> Unit) {
    Column(
        modifier = Modifier.fillMaxWidth().padding(padding).padding(16.dp),
        verticalArrangement = Arrangement.spacedBy(12.dp),
        content = content,
    )
}

@Composable
private fun BackHeader(title: String, onBack: () -> Unit) {
    Row(verticalAlignment = Alignment.CenterVertically) {
        Button(onClick = onBack) { Text("Back") }
        Spacer(Modifier.width(12.dp))
        Text(title, style = MaterialTheme.typography.headlineSmall)
    }
}

@Composable
private fun BluetoothPermissionSection() {
    val context = LocalContext.current
    val launcher = rememberLauncherForActivityResult(ActivityResultContracts.RequestMultiplePermissions()) {}
    val permissions = listOf(
        android.Manifest.permission.BLUETOOTH_SCAN,
        android.Manifest.permission.BLUETOOTH_CONNECT,
    )
    val missingPermissions = permissions.filter {
        ContextCompat.checkSelfPermission(context, it) != PackageManager.PERMISSION_GRANTED
    }
    if (missingPermissions.isNotEmpty()) {
        Button(onClick = { launcher.launch(missingPermissions.toTypedArray()) }) {
            Text("Grant Bluetooth permissions")
        }
    }
}

@Composable
private fun LocationPermissionSection(appState: CompanionAppState) {
    val context = LocalContext.current
    val launcher = rememberLauncherForActivityResult(ActivityResultContracts.RequestMultiplePermissions()) { results ->
        if (results.values.any { it }) appState.startLocationUpdates()
    }
    val permissions = listOf(
        android.Manifest.permission.ACCESS_FINE_LOCATION,
        android.Manifest.permission.ACCESS_COARSE_LOCATION,
    )
    val granted = permissions.any {
        ContextCompat.checkSelfPermission(context, it) == PackageManager.PERMISSION_GRANTED
    }
    if (!granted) {
        Button(onClick = { launcher.launch(permissions.toTypedArray()) }) {
            Text("Allow location for route planning")
        }
    }
}

private suspend fun fitCameraToRoute(
    cameraPositionState: com.google.maps.android.compose.CameraPositionState,
    coordinates: List<CoordinatePoint>,
) {
    if (coordinates.isEmpty()) return
    if (coordinates.size == 1) {
        cameraPositionState.animate(CameraUpdateFactory.newLatLngZoom(coordinates.first().toLatLng(), 14f))
        return
    }
    val boundsBuilder = LatLngBounds.builder()
    coordinates.forEach { boundsBuilder.include(it.toLatLng()) }
    cameraPositionState.animate(CameraUpdateFactory.newLatLngBounds(boundsBuilder.build(), 120))
}

private suspend fun orientCameraForTravel(
    cameraPositionState: com.google.maps.android.compose.CameraPositionState,
    coordinates: List<CoordinatePoint>,
    rider: CoordinatePoint? = null,
    bearingDegrees: Double? = null,
    ridingZoom: Float = 16f,
) {
    if (coordinates.size < 2) {
        fitCameraToRoute(cameraPositionState, coordinates)
        return
    }
    // Spec line 101: anchor on the rider's current position and rotate
    // toward the segment they're riding towards. `ridingZoom` is sourced
    // from CompanionSettings so the on-map zoom +/- buttons can persist
    // a user's preferred navigation scale across rides.
    val anchor = rider ?: coordinates.first()
    val bearing = bearingDegrees
        ?: bearingDegrees(coordinates.first(), coordinates[1])
    cameraPositionState.animate(
        CameraUpdateFactory.newCameraPosition(
            CameraPosition.Builder()
                .target(anchor.toLatLng())
                .zoom(ridingZoom)
                .bearing(bearing.toFloat())
                .tilt(0f)
                .build(),
        ),
    )
}

private fun CoordinatePoint.toLatLng(): LatLng = LatLng(latitude, longitude)

private fun bearingDegrees(start: CoordinatePoint, end: CoordinatePoint): Double {
    val latMeters = (end.latitude - start.latitude) * 111_320.0
    val lonMeters = (end.longitude - start.longitude) * cos(((start.latitude + end.latitude) / 2.0) * PI / 180.0) * 111_320.0
    return atan2(lonMeters, latMeters) * 180.0 / PI
}

private fun shareDebugPackage(context: android.content.Context, text: String) {
    val intent = Intent(Intent.ACTION_SEND)
        .setType("text/plain")
        .putExtra(Intent.EXTRA_TEXT, text)
    context.startActivity(Intent.createChooser(intent, "Share import diagnostics"))
}
