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
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.focus.LocalFocusManager
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
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

    LaunchedEffect(homeState.displayedRouteCoordinates, homeState.homeMode, homeState.compassMode) {
        val coordinates = homeState.displayedRouteCoordinates
        if (coordinates.isEmpty()) return@LaunchedEffect
        when (homeState.homeMode) {
            HomeMode.PLANNING, HomeMode.DEVICE_OVERVIEW, HomeMode.SENDING_TO_DEVICE -> fitCameraToRoute(cameraPositionState, coordinates)
            HomeMode.PHONE_GUIDANCE -> when (homeState.compassMode) {
                HomeCompassMode.AUTO_FOLLOW -> orientCameraForTravel(cameraPositionState, coordinates)
                HomeCompassMode.NORTH_PREVIEW, HomeCompassMode.NORTH_LOCKED -> fitCameraToRoute(cameraPositionState, coordinates)
            }
        }
    }

    Box(Modifier.fillMaxSize()) {
        GoogleMap(
            modifier = Modifier.fillMaxSize(),
            cameraPositionState = cameraPositionState,
            uiSettings = MapUiSettings(compassEnabled = true, myLocationButtonEnabled = false),
            properties = MapProperties(isMyLocationEnabled = false),
            onMapLongClick = { latLng ->
                if (homeState.homeMode == HomeMode.PLANNING) {
                    homeState.setDestinationFromMap(CoordinatePoint(latLng.latitude, latLng.longitude), scope)
                }
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
                HomeMode.PHONE_GUIDANCE -> PhoneGuidanceTopArea(homeState, scope)
                HomeMode.SENDING_TO_DEVICE, HomeMode.DEVICE_OVERVIEW -> DeviceOverviewTopArea(homeState, onOpenSettings)
            }

            Spacer(Modifier.weight(1f))

            when (homeState.homeMode) {
                HomeMode.PLANNING -> if (homeState.previewAlternatives.isNotEmpty()) RouteSuggestionsCard(appState, homeState)
                HomeMode.PHONE_GUIDANCE -> ActiveGuidanceCard(homeState)
                HomeMode.SENDING_TO_DEVICE, HomeMode.DEVICE_OVERVIEW -> DeviceOverviewCard(appState, homeState)
            }
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
    Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
        Row(verticalAlignment = Alignment.CenterVertically) {
            TextField(
                value = homeState.query,
                onValueChange = {
                    homeState.openSearch()
                    homeState.updateQuery(it, scope)
                },
                label = { Text("Where to?") },
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

@Composable
private fun PhoneGuidanceTopArea(homeState: HomeStateHolder, scope: kotlinx.coroutines.CoroutineScope) {
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
    val route = homeState.guidanceRoute ?: return
    Column(
        modifier = Modifier.fillMaxWidth().background(MaterialTheme.colorScheme.surface.copy(alpha = 0.94f), shape = MaterialTheme.shapes.extraLarge).padding(16.dp),
        verticalArrangement = Arrangement.spacedBy(8.dp),
    ) {
        Text(route.summary.destinationLabel ?: "Guidance active", style = MaterialTheme.typography.titleMedium)
        Text(homeState.nextInstructionLine ?: route.summaryLine, style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
        Button(onClick = { homeState.stopActiveNavigation() }, modifier = Modifier.fillMaxWidth()) {
            Text("Stop")
        }
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
        Text("Default route source")
        RouteSourceMode.entries.forEach { mode ->
            FilterChip(
                selected = appState.routePlannerPreferences.defaultSourceMode == mode,
                onClick = {
                    appState.saveRoutePlannerPreferences(appState.routePlannerPreferences.copy(defaultSourceMode = mode))
                    appState.currentSourceMode = mode
                },
                label = { Text(mode.displayName) },
            )
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
) {
    if (coordinates.size < 2) {
        fitCameraToRoute(cameraPositionState, coordinates)
        return
    }
    val anchor = coordinates.first()
    val next = coordinates[1]
    cameraPositionState.animate(
        CameraUpdateFactory.newCameraPosition(
            CameraPosition.Builder()
                .target(anchor.toLatLng())
                .zoom(16f)
                .bearing(bearingDegrees(anchor, next).toFloat())
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
