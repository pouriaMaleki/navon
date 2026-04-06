package me.fiksu.esp32map.companion

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
import androidx.compose.material3.FilterChip
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.material3.TextField
import androidx.compose.runtime.Composable
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.launch
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.core.content.ContextCompat
import androidx.lifecycle.viewmodel.compose.viewModel
import com.google.android.gms.maps.CameraUpdateFactory
import com.google.android.gms.maps.model.CameraPosition
import com.google.android.gms.maps.model.LatLng
import com.google.maps.android.compose.GoogleMap
import com.google.maps.android.compose.MapProperties
import com.google.maps.android.compose.MapUiSettings
import com.google.maps.android.compose.Marker
import com.google.maps.android.compose.MarkerState
import com.google.maps.android.compose.Polyline
import com.google.maps.android.compose.rememberCameraPositionState
import me.fiksu.esp32map.companion.app.CompanionAppState
import me.fiksu.esp32map.companion.domain.RouteHistoryItem
import me.fiksu.esp32map.companion.domain.RouteProviderId
import me.fiksu.esp32map.companion.domain.RouteStartBehavior
import me.fiksu.esp32map.companion.domain.RouteSuggestionMode
import me.fiksu.esp32map.companion.feature.home.HomeStateHolder
import me.fiksu.esp32map.companion.integration.AndroidPlaceSearchService

class MainActivity : ComponentActivity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        enableEdgeToEdge()
        setContent {
            MaterialTheme {
                CompanionApp()
            }
        }
    }
}

private enum class SettingsDestination {
    ROOT,
    CONNECTIONS,
    ROUTES,
    DEVICE,
    ROUTE_PLANNER,
}

@Composable
private fun CompanionApp(appState: CompanionAppState = viewModel()) {
    val context = LocalContext.current
    val scope = rememberCoroutineScope()
    val homeState = remember(appState) { HomeStateHolder(appState, AndroidPlaceSearchService(context)) }
    var showingSettings by rememberSaveable { mutableStateOf(false) }
    var settingsDestination by rememberSaveable { mutableStateOf(SettingsDestination.ROOT) }
    var selectedRouteId by rememberSaveable { mutableStateOf<String?>(null) }

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
                        onConnections = { settingsDestination = SettingsDestination.CONNECTIONS },
                        onRoutes = { settingsDestination = SettingsDestination.ROUTES },
                        onDevice = { settingsDestination = SettingsDestination.DEVICE },
                        onRoutePlanner = { settingsDestination = SettingsDestination.ROUTE_PLANNER },
                    )
                    SettingsDestination.CONNECTIONS -> ConnectionsSettingsScreen(onBack = { settingsDestination = SettingsDestination.ROOT })
                    SettingsDestination.ROUTES -> RoutesSettingsScreen(
                        appState = appState,
                        onBack = { settingsDestination = SettingsDestination.ROOT },
                        onOpenRoute = { selectedRouteId = it },
                    )
                    SettingsDestination.DEVICE -> DeviceSettingsScreen(appState = appState, onBack = { settingsDestination = SettingsDestination.ROOT })
                    SettingsDestination.ROUTE_PLANNER -> RoutePlannerSettingsScreen(appState = appState, onBack = { settingsDestination = SettingsDestination.ROOT })
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
                        onOpen = { openRouteItem(appState, item); selectedRouteId = null },
                        onStart = { openRouteItem(appState, item); homeState.startSelectedRoute(); selectedRouteId = null },
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

    Box(Modifier.fillMaxSize()) {
        GoogleMap(
            modifier = Modifier.fillMaxSize(),
            cameraPositionState = cameraPositionState,
            uiSettings = MapUiSettings(compassEnabled = true, myLocationButtonEnabled = false),
            properties = MapProperties(isMyLocationEnabled = false),
            onMapLongClick = { latLng ->
                homeState.setDestinationFromMap(me.fiksu.esp32map.companion.domain.CoordinatePoint(latLng.latitude, latLng.longitude), scope)
                scope.launch {
                    cameraPositionState.animate(CameraUpdateFactory.newLatLng(latLng))
                }
            },
        ) {
            homeState.previewAlternatives.forEach { alternative ->
                Polyline(
                    points = alternative.normalizedPackage.geometry.map { LatLng(it.latitude, it.longitude) },
                    color = if (alternative.id == appState.preview.selectedAlternativeId) Color(0xFF2D6CDF) else Color(0x6626A69A),
                    width = if (alternative.id == appState.preview.selectedAlternativeId) 10f else 7f,
                )
            }
            homeState.activeRoute?.let { active ->
                Polyline(
                    points = active.geometry.map { LatLng(it.latitude, it.longitude) },
                    color = Color(0xFF2E7D32),
                    width = 12f,
                )
            }
            homeState.destinationCoordinate?.let {
                Marker(state = MarkerState(LatLng(it.latitude, it.longitude)), title = "Destination")
            }
        }

        Column(modifier = Modifier.fillMaxSize().padding(16.dp)) {
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
                    Text("Settings")
                }
            }

            if (homeState.isSearchOpen) {
                Surface(
                    modifier = Modifier.fillMaxWidth().padding(top = 8.dp),
                    tonalElevation = 4.dp,
                    shape = MaterialTheme.shapes.large,
                ) {
                    LazyColumn(modifier = Modifier.height(320.dp), contentPadding = PaddingValues(vertical = 8.dp)) {
                        if (homeState.query.isBlank()) {
                            items(homeState.recentItems, key = { it.id }) { item ->
                                RouteHistoryRow(item = item, onClick = { homeState.selectRecent(item, scope) })
                                homeState.loadMoreRecentsIfNeeded(item)
                            }
                        } else {
                            items(homeState.visibleSuggestions, key = { it.id }) { suggestion ->
                                SearchSuggestionRow(
                                    title = suggestion.title,
                                    subtitle = suggestion.subtitle,
                                    onClick = {
                                        homeState.selectSuggestion(suggestion, scope)
                                        scope.launch {
                                            cameraPositionState.animate(CameraUpdateFactory.newLatLng(LatLng(suggestion.coordinate.latitude, suggestion.coordinate.longitude)))
                                        }
                                    },
                                )
                                homeState.loadMoreSuggestionsIfNeeded(suggestion)
                            }
                        }
                    }
                }
            }

            Spacer(Modifier.weight(1f))

            when {
                homeState.activeRoute != null -> ActiveGuidanceCard(homeState, scope)
                homeState.previewAlternatives.isNotEmpty() -> RouteSuggestionsCard(appState, homeState)
            }
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
        Text("Suggested routes", style = MaterialTheme.typography.titleMedium)
        homeState.previewAlternatives.forEach { alternative ->
            val selected = alternative.id == appState.preview.selectedAlternativeId
            Row(
                modifier = Modifier.fillMaxWidth().clickable { homeState.selectAlternative(alternative.id) }.background(if (selected) MaterialTheme.colorScheme.primary.copy(alpha = 0.12f) else Color.Transparent, shape = MaterialTheme.shapes.medium).padding(12.dp),
                verticalAlignment = Alignment.CenterVertically,
            ) {
                Column(Modifier.weight(1f)) {
                    Text(alternative.title, fontWeight = FontWeight.SemiBold)
                    Text(alternative.normalizedPackage.summaryLine, style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
                }
                if (selected) {
                    Text("Selected", color = MaterialTheme.colorScheme.primary, style = MaterialTheme.typography.labelMedium)
                }
            }
        }
        Button(onClick = { homeState.startSelectedRoute() }, modifier = Modifier.fillMaxWidth()) {
            Text("Start")
        }
    }
}

@Composable
private fun ActiveGuidanceCard(homeState: HomeStateHolder, scope: CoroutineScope) {
    val route = homeState.activeRoute ?: return
    Column(
        modifier = Modifier.fillMaxWidth().background(MaterialTheme.colorScheme.surface.copy(alpha = 0.94f), shape = MaterialTheme.shapes.extraLarge).padding(16.dp),
        verticalArrangement = Arrangement.spacedBy(8.dp),
    ) {
        Text(route.summary.destinationLabel ?: "Guidance active", style = MaterialTheme.typography.titleMedium)
        Text(route.summaryLine, style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
        Button(onClick = { homeState.stopGuidance(scope) }, modifier = Modifier.fillMaxWidth()) {
            Text("Stop")
        }
    }
}

@Composable
private fun SettingsRootScreen(
    onDismiss: () -> Unit,
    onConnections: () -> Unit,
    onRoutes: () -> Unit,
    onDevice: () -> Unit,
    onRoutePlanner: () -> Unit,
) {
    ScreenColumn(PaddingValues(0.dp)) {
        Row(verticalAlignment = Alignment.CenterVertically) {
            Text("Settings", style = MaterialTheme.typography.headlineSmall, modifier = Modifier.weight(1f))
            Button(onClick = onDismiss) { Text("Done") }
        }
        Button(onClick = onConnections, modifier = Modifier.fillMaxWidth()) { Text("Connections") }
        Button(onClick = onRoutes, modifier = Modifier.fillMaxWidth()) { Text("Routes") }
        Button(onClick = onDevice, modifier = Modifier.fillMaxWidth()) { Text("Device") }
        Button(onClick = onRoutePlanner, modifier = Modifier.fillMaxWidth()) { Text("Route Planner") }
    }
}

@Composable
private fun ConnectionsSettingsScreen(onBack: () -> Unit) {
    ScreenColumn(PaddingValues(0.dp)) {
        BackHeader(title = "Connections", onBack = onBack)
        ConnectionRow("Strava", "Inbound route integration planned")
        ConnectionRow("Garmin Connect", "Inbound route integration planned")
        ConnectionRow("Komoot", "Inbound route integration planned")
    }
}

@Composable
private fun ConnectionRow(title: String, subtitle: String) {
    Column(Modifier.fillMaxWidth().padding(vertical = 8.dp), verticalArrangement = Arrangement.spacedBy(4.dp)) {
        Text(title, fontWeight = FontWeight.SemiBold)
        Text(subtitle, style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
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
        Button(onClick = appState::sendSelectedRoute) { Text("Send selected route") }
        Button(onClick = appState::resumePendingTransfer) { Text("Resume pending transfer") }
        Button(onClick = appState::armRetryableInterruptionOnNextTransfer) { Text("Arm retryable interruption") }
        Button(onClick = appState::armWriteFailureOnNextTransfer) { Text("Arm write failure") }
        Button(onClick = appState::armDisconnectAfterNextChunkWrite) { Text("Arm disconnect after chunk") }
        Button(onClick = appState::armDropNextInboundStatus) { Text("Arm drop next inbound status") }
        Button(onClick = appState::clearActiveRoute) { Text("Clear active route") }
    }
}

@Composable
private fun RoutePlannerSettingsScreen(appState: CompanionAppState, onBack: () -> Unit) {
    ScreenColumn(PaddingValues(0.dp)) {
        BackHeader(title = "Route Planner", onBack = onBack)
        Text("Provider")
        RouteProviderId.entries.forEach { provider ->
            FilterChip(
                selected = appState.routePlannerPreferences.providerId == provider,
                onClick = {
                    appState.saveRoutePlannerPreferences(appState.routePlannerPreferences.copy(providerId = provider))
                    appState.selectedProviderId = provider
                },
                label = { Text(provider.displayName) },
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

private fun openRouteItem(appState: CompanionAppState, item: RouteHistoryItem) {
    item.routePackage?.let { routePackage ->
        appState.preview = appState.preview.copy(
            alternatives = listOf(
                me.fiksu.esp32map.companion.domain.RouteAlternative(
                    id = item.id,
                    title = item.title,
                    subtitle = item.subtitle,
                    distanceMeters = routePackage.summary.totalDistanceMeters.toInt(),
                    durationSeconds = routePackage.summary.estimatedDurationSeconds,
                    normalizedPackage = routePackage,
                ),
            ),
            selectedAlternativeId = item.id,
            routeIdentifier = routePackage.routeIdentifier,
            routeRevision = routePackage.revision,
            planningNotice = item.sourceLabel,
        )
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
