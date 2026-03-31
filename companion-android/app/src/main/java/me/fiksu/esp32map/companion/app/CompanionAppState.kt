package me.fiksu.esp32map.companion.app

import android.app.Application
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import androidx.lifecycle.AndroidViewModel
import androidx.lifecycle.viewModelScope
import kotlinx.coroutines.flow.collectLatest
import kotlinx.coroutines.launch
import me.fiksu.esp32map.companion.domain.ActiveRouteSession
import me.fiksu.esp32map.companion.domain.CompanionSettings
import me.fiksu.esp32map.companion.domain.CoordinatePoint
import me.fiksu.esp32map.companion.domain.RoutePlanRequest
import me.fiksu.esp32map.companion.domain.RoutePreviewModel
import me.fiksu.esp32map.companion.domain.RouteProviderId
import me.fiksu.esp32map.companion.domain.RouteRerouteRequestMessage
import me.fiksu.esp32map.companion.domain.RoutingProvider
import me.fiksu.esp32map.companion.domain.SyncSessionState
import me.fiksu.esp32map.companion.integration.ble.BleRouteSyncService
import me.fiksu.esp32map.companion.integration.diagnostics.CompanionDiagnosticsStore
import me.fiksu.esp32map.companion.integration.hsl.HslRoutingAdapter
import me.fiksu.esp32map.companion.integration.persistence.CompanionPersistence
import me.fiksu.esp32map.companion.integration.sample.SampleRoutingAdapter

class CompanionAppState(application: Application) : AndroidViewModel(application) {
    var selectedProviderId by mutableStateOf(RouteProviderId.HSL)
    var settings by mutableStateOf(CompanionSettings())
    var simulatedRiderLocation by mutableStateOf(CoordinatePoint(60.1699, 24.9384))
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
    val bleService = BleRouteSyncService(application.applicationContext)

    private val providers: Map<RouteProviderId, RoutingProvider> = mapOf(
        RouteProviderId.HSL to HslRoutingAdapter(settingsProvider = { settings }),
        RouteProviderId.OSM to SampleRoutingAdapter(RouteProviderId.OSM),
        RouteProviderId.GOOGLE_INGEST to SampleRoutingAdapter(RouteProviderId.GOOGLE_INGEST),
        RouteProviderId.GPX_IMPORT to SampleRoutingAdapter(RouteProviderId.GPX_IMPORT),
        RouteProviderId.FIT_IMPORT to SampleRoutingAdapter(RouteProviderId.FIT_IMPORT),
        RouteProviderId.TCX_IMPORT to SampleRoutingAdapter(RouteProviderId.TCX_IMPORT),
        RouteProviderId.GARMIN_API to SampleRoutingAdapter(RouteProviderId.GARMIN_API),
        RouteProviderId.GARMIN_FILE to SampleRoutingAdapter(RouteProviderId.GARMIN_FILE),
    )

    val selectedProviderCanPlan: Boolean
        get() = providers[selectedProviderId] != null

    init {
        settings = persistence.loadSettings()
        viewModelScope.launch {
            bleService.state.collectLatest {
                syncSession = it
                refreshDiagnostics()
            }
        }
    }

    fun persistSettings() {
        persistence.saveSettings(settings)
    }

    fun planRoute() {
        val provider = providers[selectedProviderId] ?: return
        routeRequest = routeRequest.copy(providerId = selectedProviderId)
        viewModelScope.launch {
            preview = provider.planRoute(routeRequest)
            applySelectedAlternativeToSession(provider.providerId, routeRequest.destination)
            simulatedRiderLocation = routeRequest.origin
            persistence.saveRecentDestination(routeRequest.destination)
            refreshDiagnostics()
        }
    }

    fun sendSelectedRoute() {
        val provider = providers[selectedProviderId] ?: return
        viewModelScope.launch {
            val normalized = provider.normalizePreview(preview, routeRequest)
            val shouldUpdate = syncSession.activeRouteIdentifier == normalized.routeIdentifier && syncSession.activeRouteRevision != null
            if (shouldUpdate) {
                bleService.publishUpdate(normalized)
            } else {
                bleService.publishSet(normalized)
            }
            activeSession = activeSession.copy(
                routeIdentifier = normalized.routeIdentifier,
                routeRevision = normalized.revision,
                destinationLabel = normalized.summary.destinationLabel ?: activeSession.destinationLabel,
            )
            persistence.saveSession(activeSession)
            refreshDiagnostics()
        }
    }

    fun selectAlternative(alternativeId: String) {
        preview = preview.copy(
            selectedAlternativeId = alternativeId,
            routeIdentifier = preview.alternatives.firstOrNull { it.id == alternativeId }?.normalizedPackage?.routeIdentifier,
            routeRevision = preview.alternatives.firstOrNull { it.id == alternativeId }?.normalizedPackage?.revision,
        )
        applySelectedAlternativeToSession(selectedProviderId, routeRequest.destination)
        refreshDiagnostics()
    }

    fun clearActiveRoute() {
        viewModelScope.launch {
            bleService.publishClear(activeSession.routeIdentifier)
            activeSession = activeSession.copy(
                routeIdentifier = null,
                routeRevision = null,
            )
            refreshDiagnostics()
        }
    }

    fun resumePendingTransfer() {
        viewModelScope.launch {
            bleService.resumePendingTransfer()
            refreshDiagnostics()
        }
    }

    fun armRetryableInterruptionOnNextTransfer() {
        bleService.armRetryableInterruptionOnNextTransfer()
        refreshDiagnostics()
    }

    fun armWriteFailureOnNextTransfer() {
        bleService.armFaultInjection(me.fiksu.esp32map.companion.domain.RouteSyncFaultInjectionMode.WRITE_FAILURE)
        refreshDiagnostics()
    }

    fun armDisconnectAfterNextChunkWrite() {
        bleService.armFaultInjection(me.fiksu.esp32map.companion.domain.RouteSyncFaultInjectionMode.DISCONNECT_AFTER_CHUNK_WRITE)
        refreshDiagnostics()
    }

    fun armDropNextInboundStatus() {
        bleService.armFaultInjection(me.fiksu.esp32map.companion.domain.RouteSyncFaultInjectionMode.DROP_NEXT_INBOUND_STATUS)
        refreshDiagnostics()
    }

    fun connectToDevice() {
        viewModelScope.launch {
            bleService.scanForDevices()
            bleService.connectToLastKnownDevice()
            refreshDiagnostics()
        }
    }

    fun triggerReroute() {
        val provider = providers[selectedProviderId] ?: return
        viewModelScope.launch {
            val routeIdentifier = activeSession.routeIdentifier ?: "preview-route"
            val riderLocation = simulatedRiderLocation
            bleService.receiveRerouteRequest(
                RouteRerouteRequestMessage(
                    routeIdentifier = routeIdentifier,
                    riderLocation = riderLocation,
                    reason = "User drifted off route",
                ),
            )
            preview = provider.replanRoute(activeSession, riderLocation)
            applySelectedAlternativeToSession(provider.providerId, activeSession.destinationCoordinate ?: routeRequest.destination)
            activeSession = activeSession.copy(
                routeRevision = preview.selectedAlternative?.normalizedPackage?.revision ?: ((activeSession.routeRevision ?: 0) + 1),
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

    private fun applySelectedAlternativeToSession(providerId: RouteProviderId, destination: CoordinatePoint) {
        val selectedPackage = preview.selectedAlternative?.normalizedPackage
        activeSession = activeSession.copy(
            routeIdentifier = selectedPackage?.routeIdentifier ?: preview.routeIdentifier,
            routeRevision = selectedPackage?.revision ?: preview.routeRevision,
            destinationLabel = selectedPackage?.summary?.destinationLabel ?: providerId.displayName + " route",
            destinationCoordinate = destination,
            providerId = providerId,
        )
    }
}
