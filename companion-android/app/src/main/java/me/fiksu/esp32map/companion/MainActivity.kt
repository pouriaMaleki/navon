package me.fiksu.esp32map.companion

import android.os.Bundle
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.activity.enableEdgeToEdge
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ColumnScope
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.material3.Button
import androidx.compose.material3.FilterChip
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.NavigationBar
import androidx.compose.material3.NavigationBarItem
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Text
import androidx.compose.material3.TextField
import androidx.compose.runtime.Composable
import androidx.compose.runtime.State
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp
import androidx.lifecycle.viewmodel.compose.viewModel
import kotlinx.coroutines.flow.StateFlow
import me.fiksu.esp32map.companion.app.CompanionAppState
import me.fiksu.esp32map.companion.domain.RouteProviderId

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

private enum class CompanionTab(val label: String) {
    Launch("Launch"),
    Plan("Plan"),
    Preview("Preview"),
    Device("Device"),
    Ride("Ride"),
    Settings("Settings"),
}

@Composable
private fun CompanionApp(appState: CompanionAppState = viewModel()) {
    var selectedTab by remember { mutableStateOf(CompanionTab.Launch) }

    Scaffold(
        modifier = Modifier.fillMaxSize(),
        bottomBar = {
            NavigationBar {
                CompanionTab.entries.forEach { tab ->
                    NavigationBarItem(
                        selected = selectedTab == tab,
                        onClick = { selectedTab = tab },
                        icon = { Text(tab.label.take(1)) },
                        label = { Text(tab.label) },
                    )
                }
            }
        },
    ) { innerPadding ->
        when (selectedTab) {
            CompanionTab.Launch -> LaunchScreen(innerPadding, appState)
            CompanionTab.Plan -> RoutePlanningScreen(innerPadding, appState)
            CompanionTab.Preview -> RoutePreviewScreen(innerPadding, appState)
            CompanionTab.Device -> DeviceScreen(innerPadding, appState)
            CompanionTab.Ride -> ActiveRideScreen(innerPadding, appState)
            CompanionTab.Settings -> SettingsScreen(innerPadding, appState)
        }
    }
}

@Composable
private fun LaunchScreen(padding: PaddingValues, appState: CompanionAppState) {
    ScreenColumn(padding) {
        Text("BLE state: ${appState.syncSession.connectionState}")
        Text("Last device: ${appState.syncSession.lastDeviceName ?: "None"}")
        Text("Route: ${appState.activeSession.routeIdentifier ?: "None"}")
        Button(onClick = appState::connectToDevice) {
            Text("Connect to device")
        }
    }
}

@Composable
private fun RoutePlanningScreen(padding: PaddingValues, appState: CompanionAppState) {
    ScreenColumn(padding) {
        Text("Provider")
        RouteProviderId.entries.forEach { provider ->
            FilterChip(
                selected = appState.selectedProviderId == provider,
                onClick = { appState.selectedProviderId = provider },
                enabled = provider.isAvailableInV1,
                label = {
                    Text(if (provider.isAvailableInV1) provider.displayName else "${provider.displayName} (Coming soon)")
                },
            )
        }

        CoordinateField("Origin latitude", appState.routeRequest.origin.latitude) {
            appState.routeRequest = appState.routeRequest.copy(origin = appState.routeRequest.origin.copy(latitude = it))
        }
        CoordinateField("Origin longitude", appState.routeRequest.origin.longitude) {
            appState.routeRequest = appState.routeRequest.copy(origin = appState.routeRequest.origin.copy(longitude = it))
        }
        CoordinateField("Destination latitude", appState.routeRequest.destination.latitude) {
            appState.routeRequest = appState.routeRequest.copy(destination = appState.routeRequest.destination.copy(latitude = it))
        }
        CoordinateField("Destination longitude", appState.routeRequest.destination.longitude) {
            appState.routeRequest = appState.routeRequest.copy(destination = appState.routeRequest.destination.copy(longitude = it))
        }

        Button(
            onClick = appState::planRoute,
            enabled = appState.selectedProviderId.isAvailableInV1,
        ) {
            Text("Plan HSL route")
        }
    }
}

@Composable
private fun RoutePreviewScreen(padding: PaddingValues, appState: CompanionAppState) {
    ScreenColumn(padding) {
        Text("Route id: ${appState.preview.routeIdentifier ?: "None"}")
        Text("Revision: ${appState.preview.routeRevision ?: 0}")
        LazyColumn(verticalArrangement = Arrangement.spacedBy(8.dp)) {
            items(appState.preview.alternatives) { alternative ->
                Column {
                    Text(alternative.title, style = MaterialTheme.typography.titleMedium)
                    Text(alternative.subtitle)
                    Text(alternative.normalizedPackage.summaryLine)
                    Text("${alternative.normalizedPackage.geometryPointCount} geometry points • ${alternative.normalizedPackage.maneuverCount} maneuvers")
                }
            }
        }
        appState.preview.selectedAlternative?.normalizedPackage?.let { selected ->
            Text("Provenance: ${selected.provenance.providerId.displayName}")
            Text("Source: ${selected.provenance.sourceReference ?: "None"}")
        }
        Button(onClick = appState::sendSelectedRoute, enabled = appState.preview.routeIdentifier != null) {
            Text("Send to device")
        }
    }
}

@Composable
private fun DeviceScreen(padding: PaddingValues, appState: CompanionAppState) {
    ScreenColumn(padding) {
        Text("Connection: ${appState.syncSession.connectionState}")
        Text("Route sync: ${appState.syncSession.routeSyncState}")
        Text("Active route: ${appState.syncSession.activeRouteIdentifier ?: "None"}")
        Text("Active revision: ${appState.syncSession.activeRouteRevision ?: 0}")
        Text("Last sync: ${appState.syncSession.lastSyncResult}")
        Text("Outbound: ${appState.syncSession.lastOutboundMessage?.debugSummary ?: "None"}")
        Text("Inbound: ${appState.syncSession.lastInboundMessage?.debugSummary ?: "None"}")
        Text("Status code: ${appState.syncSession.lastStatusCode?.name ?: "NONE"}")
        Button(onClick = appState::connectToDevice) {
            Text("Reconnect")
        }
        Button(onClick = appState::sendSelectedRoute) {
            Text("Send route message")
        }
        Button(onClick = appState::clearActiveRoute) {
            Text("Clear active route")
        }
        Button(onClick = appState::triggerDemoReroute) {
            Text("Simulate reroute request")
        }
    }
}

@Composable
private fun ActiveRideScreen(padding: PaddingValues, appState: CompanionAppState) {
    ScreenColumn(padding) {
        Text("Route id: ${appState.activeSession.routeIdentifier ?: "None"}")
        Text("Revision: ${appState.activeSession.routeRevision ?: 0}")
        Text("Destination: ${appState.activeSession.destinationLabel}")
        Text("Provider: ${appState.activeSession.providerId.displayName}")
        appState.activeSession.destinationCoordinate?.let { destination ->
            Text("Destination lat/lon: %.5f, %.5f".format(destination.latitude, destination.longitude))
        }
        Text("Last reroute: ${appState.activeSession.lastRerouteReason ?: "No reroute yet"}")
        Text("Reroute time: ${appState.activeSession.lastRerouteTimestamp ?: "Never"}")
    }
}

@Composable
private fun SettingsScreen(padding: PaddingValues, appState: CompanionAppState) {
    val diagnostics by appState.diagnosticsStore.state.collectAsStateCompat()
    ScreenColumn(padding) {
        Text("Diagnostics")
        Text("Provider: ${diagnostics.providerName}")
        Text("Route: ${diagnostics.routeIdentifier}")
        Text("Revision: ${diagnostics.routeRevision}")
        Text("BLE: ${diagnostics.bleState}")
        Text("Sync: ${diagnostics.lastSyncResult}")
        Text("Reroute: ${diagnostics.lastRerouteOutcome}")
        Text("Last outbound kind: ${appState.syncSession.lastOutboundMessage?.kindLabel ?: "none"}")
        Text("Last inbound kind: ${appState.syncSession.lastInboundMessage?.kindLabel ?: "none"}")
    }
}

@Composable
private fun ScreenColumn(
    padding: PaddingValues,
    content: @Composable ColumnScope.() -> Unit,
) {
    Column(
        modifier = Modifier
            .fillMaxSize()
            .padding(padding)
            .padding(16.dp),
        verticalArrangement = Arrangement.spacedBy(12.dp),
        content = content,
    )
}

@Composable
private fun CoordinateField(label: String, value: Double, onValueChange: (Double) -> Unit) {
    var text by remember(value) { mutableStateOf(value.toString()) }
    TextField(
        value = text,
        onValueChange = {
            text = it
            it.toDoubleOrNull()?.let(onValueChange)
        },
        label = { Text(label) },
        modifier = Modifier.fillMaxWidth(),
    )
}

@Composable
private fun <T> StateFlow<T>.collectAsStateCompat(): State<T> {
    return collectAsState(initial = value)
}
