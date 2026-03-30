package me.fiksu.esp32map.companion.app

import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import kotlinx.coroutines.flow.collectLatest
import kotlinx.coroutines.launch
import me.fiksu.esp32map.companion.domain.ActiveRouteSession
import me.fiksu.esp32map.companion.domain.CoordinatePoint
import me.fiksu.esp32map.companion.domain.RoutePlanRequest
import me.fiksu.esp32map.companion.domain.RoutePreviewModel
import me.fiksu.esp32map.companion.domain.RouteProviderId
import me.fiksu.esp32map.companion.domain.RoutingProvider
import me.fiksu.esp32map.companion.domain.SyncSessionState
import me.fiksu.esp32map.companion.integration.ble.BleRouteSyncService
import me.fiksu.esp32map.companion.integration.diagnostics.CompanionDiagnosticsStore
import me.fiksu.esp32map.companion.integration.hsl.HslRoutingAdapter
import me.fiksu.esp32map.companion.integration.persistence.CompanionPersistence

class CompanionAppState : ViewModel() {
    var selectedProviderId by mutableStateOf(RouteProviderId.HSL)
    var routeRequest by mutableStateOf(
        RoutePlanRequest(
            origin = CoordinatePoint(60.1699, 24.9384),
            destination = CoordinatePoint(60.1921, 24.9458),
            providerId = RouteProviderId.HSL,
        ),
    )
    var preview by mutableStateOf(RoutePreviewModel())
    var activeSession by mutableStateOf(ActiveRouteSession())
    var syncSession by mutableStateOf(SyncSessionState())

    val diagnosticsStore = CompanionDiagnosticsStore()
    val persistence = CompanionPersistence()
    val bleService = BleRouteSyncService()

    private val providers: Map<RouteProviderId, RoutingProvider> = mapOf(
        RouteProviderId.HSL to HslRoutingAdapter(),
    )

    init {
        viewModelScope.launch {
            bleService.state.collectLatest {
                syncSession = it
                refreshDiagnostics()
            }
        }
    }

    fun planRoute() {
        val provider = providers[selectedProviderId] ?: return
        routeRequest = routeRequest.copy(providerId = selectedProviderId)
        viewModelScope.launch {
            preview = provider.planRoute(routeRequest)
            val selectedPackage = preview.selectedAlternative?.normalizedPackage
            activeSession = activeSession.copy(
                routeIdentifier = selectedPackage?.routeIdentifier ?: preview.routeIdentifier,
                routeRevision = selectedPackage?.revision ?: preview.routeRevision,
                destinationLabel = selectedPackage?.summary?.destinationLabel ?: provider.providerId.displayName + " route",
                destinationCoordinate = routeRequest.destination,
                providerId = provider.providerId,
            )
            persistence.saveRecentDestination(routeRequest.destination)
            refreshDiagnostics()
        }
    }

    fun sendSelectedRoute() {
        val provider = providers[selectedProviderId] ?: return
        viewModelScope.launch {
            val normalized = provider.normalizePreview(preview, routeRequest)
            bleService.sendRoute(normalized)
            activeSession = activeSession.copy(
                routeIdentifier = normalized.routeIdentifier,
                routeRevision = normalized.revision,
                destinationLabel = normalized.summary.destinationLabel ?: activeSession.destinationLabel,
            )
            persistence.saveSession(activeSession)
            refreshDiagnostics()
        }
    }

    fun connectToDevice() {
        viewModelScope.launch {
            bleService.scanForDevices()
            bleService.connectToLastKnownDevice()
            refreshDiagnostics()
        }
    }

    fun triggerDemoReroute() {
        val provider = providers[selectedProviderId] ?: return
        viewModelScope.launch {
            preview = provider.replanRoute(activeSession, routeRequest.origin)
            activeSession = activeSession.copy(
                routeRevision = (activeSession.routeRevision ?: 0) + 1,
                lastRerouteReason = "Device requested reroute",
                lastRerouteTimestamp = "Just now",
            )
            sendSelectedRoute()
        }
    }

    fun refreshDiagnostics() {
        diagnosticsStore.update(
            session = activeSession.takeIf { it.routeIdentifier != null },
            syncState = syncSession,
        )
    }
}
