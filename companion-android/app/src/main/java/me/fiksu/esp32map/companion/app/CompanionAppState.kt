package me.fiksu.esp32map.companion.app

import android.app.Application
import android.content.Context
import android.net.Uri
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
import me.fiksu.esp32map.companion.domain.RouteAlternative
import me.fiksu.esp32map.companion.domain.RouteHistoryItem
import me.fiksu.esp32map.companion.domain.RouteHistorySource
import me.fiksu.esp32map.companion.domain.RoutePlanRequest
import me.fiksu.esp32map.companion.domain.RoutePlannerPreferences
import me.fiksu.esp32map.companion.domain.RouteProviderId
import me.fiksu.esp32map.companion.domain.RouteRerouteRequestMessage
import me.fiksu.esp32map.companion.domain.RoutePreviewModel
import me.fiksu.esp32map.companion.domain.RouteSourceMode
import me.fiksu.esp32map.companion.domain.RouteSuggestionKind
import me.fiksu.esp32map.companion.domain.RoutingProvider
import me.fiksu.esp32map.companion.domain.SyncSessionState
import me.fiksu.esp32map.companion.integration.ble.BleRouteSyncService
import me.fiksu.esp32map.companion.integration.diagnostics.CompanionDiagnosticsStore
import me.fiksu.esp32map.companion.integration.gpx.GpxRoutingAdapter
import me.fiksu.esp32map.companion.integration.hsl.HslRoutingAdapter
import me.fiksu.esp32map.companion.integration.persistence.CompanionPersistence
import me.fiksu.esp32map.companion.integration.sample.SampleRoutingAdapter

class CompanionAppState(application: Application) : AndroidViewModel(application) {
    var selectedProviderId by mutableStateOf(RouteProviderId.HSL)
    var currentSourceMode by mutableStateOf(RouteSourceMode.MIXED)
    var settings by mutableStateOf(CompanionSettings())
    var routePlannerPreferences by mutableStateOf(RoutePlannerPreferences())
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
    val persistence = CompanionPersistence(application.applicationContext)
    val bleService = BleRouteSyncService(application.applicationContext)

    private var lastHandledRerouteSignature: String? = null

    private val providers: Map<RouteProviderId, RoutingProvider> = mapOf(
        RouteProviderId.HSL to HslRoutingAdapter(settingsProvider = { settings }),
        RouteProviderId.OSM to SampleRoutingAdapter(RouteProviderId.OSM),
        RouteProviderId.GOOGLE_INGEST to SampleRoutingAdapter(RouteProviderId.GOOGLE_INGEST),
        RouteProviderId.GPX_IMPORT to GpxRoutingAdapter(),
        RouteProviderId.FIT_IMPORT to SampleRoutingAdapter(RouteProviderId.FIT_IMPORT),
        RouteProviderId.TCX_IMPORT to SampleRoutingAdapter(RouteProviderId.TCX_IMPORT),
        RouteProviderId.GARMIN_API to SampleRoutingAdapter(RouteProviderId.GARMIN_API),
        RouteProviderId.GARMIN_FILE to SampleRoutingAdapter(RouteProviderId.GARMIN_FILE),
    )

    val sourceModeOptions: List<RouteSourceMode>
        get() = RouteSourceMode.entries

    val routeHistoryItems: List<RouteHistoryItem>
        get() = persistence.loadRecentRouteHistory()

    val isDeviceConnected: Boolean
        get() = syncSession.connectionState == me.fiksu.esp32map.companion.domain.DeviceConnectionState.CONNECTED

    init {
        settings = persistence.loadSettings()
        routePlannerPreferences = persistence.loadRoutePlannerPreferences()
        currentSourceMode = routePlannerPreferences.defaultSourceMode
        persistence.loadLastSession()?.let { activeSession = it }
        selectedProviderId = activeSession.providerId
        viewModelScope.launch {
            bleService.state.collectLatest {
                syncSession = it
                refreshDiagnostics()
                val inbound = it.lastInboundMessage
                if (inbound is me.fiksu.esp32map.companion.domain.RouteSyncMessage.RerouteRequest) {
                    val message = inbound.message
                    val signature = "${message.routeIdentifier}-${message.riderLocation.latitude}-${message.riderLocation.longitude}-${message.reason}"
                    if (signature != lastHandledRerouteSignature) {
                        lastHandledRerouteSignature = signature
                        rerouteActiveSession(message.riderLocation, message.reason)
                    }
                }
            }
        }
    }

    fun persistSettings() {
        persistence.saveSettings(settings)
    }

    fun importGpxUri(context: Context, uri: Uri) {
        viewModelScope.launch {
            try {
                val bytes = context.contentResolver.openInputStream(uri)?.use { it.readBytes() }
                    ?: error("Unable to read GPX file")
                val fileName = uri.lastPathSegment?.substringAfterLast('/') ?: "route.gpx"
                val adapter = providers[RouteProviderId.GPX_IMPORT] as? GpxRoutingAdapter ?: return@launch
                preview = adapter.importBytes(fileName = fileName, data = bytes)
                selectedProviderId = RouteProviderId.GPX_IMPORT
                preview.selectedAlternative?.normalizedPackage?.let { selected ->
                    routeRequest = RoutePlanRequest(
                        origin = selected.geometry.firstOrNull() ?: routeRequest.origin,
                        destination = selected.geometry.lastOrNull() ?: routeRequest.destination,
                        providerId = RouteProviderId.GPX_IMPORT,
                    )
                    simulatedRiderLocation = selected.geometry.firstOrNull() ?: simulatedRiderLocation
                }
                applySelectedAlternativeToSession(currentSourceMode, routeRequest.destination, fileName.substringBeforeLast('.'))
                refreshDiagnostics()
            } catch (error: Exception) {
                preview = RoutePreviewModel(
                    alternatives = emptyList(),
                    selectedAlternativeId = null,
                    routeIdentifier = null,
                    routeRevision = null,
                    planningNotice = "GPX import failed: ${error.message ?: error::class.simpleName}"
                )
            }
        }
    }

    fun planRoute(sourceMode: RouteSourceMode = currentSourceMode, preferredTitle: String? = null, revisionOverride: Int? = null, onComplete: () -> Unit = {}) {
        currentSourceMode = sourceMode
        routeRequest = routeRequest.copy(providerId = sourceMode.primaryProviderId)
        viewModelScope.launch {
            runCatching {
                val nextPreview = buildPreview(routeRequest, sourceMode, revisionOverride)
                preview = nextPreview
                simulatedRiderLocation = routeRequest.origin
                persistence.saveRecentDestination(routeRequest.destination)
                applySelectedAlternativeToSession(sourceMode, routeRequest.destination, preferredTitle)
                refreshDiagnostics()
            }.onFailure { error ->
                preview = RoutePreviewModel(
                    alternatives = emptyList(),
                    selectedAlternativeId = null,
                    routeIdentifier = null,
                    routeRevision = null,
                    planningNotice = "Planning failed: ${error.message ?: error::class.simpleName}",
                )
                refreshDiagnostics()
            }
            onComplete()
        }
    }

    fun sendSelectedRoute(onComplete: (Boolean) -> Unit = {}) {
        val selected = preview.selectedAlternative ?: run {
            onComplete(false)
            return
        }
        val provider = providers[selected.normalizedPackage.provenance.providerId] ?: run {
            onComplete(false)
            return
        }
        viewModelScope.launch {
            runCatching {
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
                    destinationCoordinate = normalized.geometry.lastOrNull() ?: activeSession.destinationCoordinate,
                    providerId = normalized.provenance.providerId,
                    sourceMode = currentSourceMode,
                )
                persistence.saveSession(activeSession)
                refreshDiagnostics()
            }.onSuccess {
                onComplete(true)
            }.onFailure {
                refreshDiagnostics()
                onComplete(false)
            }
        }
    }

    fun clearActiveRoute(onComplete: (Boolean) -> Unit = {}) {
        viewModelScope.launch {
            runCatching {
                bleService.publishClear(activeSession.routeIdentifier)
                activeSession = activeSession.copy(routeIdentifier = null, routeRevision = null)
                persistence.saveSession(activeSession)
                refreshDiagnostics()
            }.onSuccess {
                onComplete(true)
            }.onFailure {
                refreshDiagnostics()
                onComplete(false)
            }
        }
    }

    fun selectAlternative(alternativeId: String) {
        preview = preview.copy(
            selectedAlternativeId = alternativeId,
            routeIdentifier = preview.alternatives.firstOrNull { it.id == alternativeId }?.normalizedPackage?.routeIdentifier,
            routeRevision = preview.alternatives.firstOrNull { it.id == alternativeId }?.normalizedPackage?.revision,
        )
        preview.selectedAlternative?.normalizedPackage?.provenance?.providerId?.let { selectedProviderId = it }
        applySelectedAlternativeToSession(currentSourceMode, routeRequest.destination, null)
        refreshDiagnostics()
    }

    fun connectToDevice() {
        viewModelScope.launch {
            bleService.scanForDevices()
            bleService.connectToLastKnownDevice()
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

    fun saveRoutePlannerPreferences(preferences: RoutePlannerPreferences) {
        routePlannerPreferences = preferences
        currentSourceMode = preferences.defaultSourceMode
        persistence.saveRoutePlannerPreferences(preferences)
    }

    fun recordPlannedPreview(source: RouteHistorySource, sourceLabel: String) {
        val selected = preview.selectedAlternative?.normalizedPackage ?: return
        persistence.saveRouteHistoryItem(
            RouteHistoryItem(
                id = selected.routeIdentifier,
                title = selected.summary.destinationLabel ?: "Route",
                subtitle = selected.summaryLine,
                source = source,
                sourceLabel = sourceLabel,
                createdAtLabel = "Just now",
                destination = selected.geometry.lastOrNull(),
                routePackage = selected,
            ),
        )
    }

    fun recordRecentDestination(title: String, coordinate: CoordinatePoint) {
        persistence.saveRouteHistoryItem(
            RouteHistoryItem(
                id = "recent-${coordinate.latitude}-${coordinate.longitude}-$title",
                title = title,
                subtitle = "Recent destination",
                source = RouteHistorySource.RECENT_DESTINATION,
                sourceLabel = "Recent",
                createdAtLabel = "Just now",
                destination = coordinate,
                routePackage = null,
            ),
        )
        persistence.saveRecentDestination(coordinate)
    }

    fun dismissRouteHistoryItem(id: String) {
        persistence.dismissRouteHistoryItem(id)
    }

    fun applyRouteHistoryPreview(item: RouteHistoryItem, onComplete: () -> Unit = {}) {
        viewModelScope.launch {
            if (item.routePackage != null) {
                val packageRef = item.routePackage
                preview = RoutePreviewModel(
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
                selectedProviderId = packageRef.provenance.providerId
                if (packageRef.provenance.providerId == RouteProviderId.OSM) {
                    currentSourceMode = RouteSourceMode.OSM
                } else if (packageRef.provenance.providerId == RouteProviderId.HSL) {
                    currentSourceMode = RouteSourceMode.HSL
                }
                routeRequest = RoutePlanRequest(
                    origin = simulatedRiderLocation,
                    destination = item.destination ?: packageRef.geometry.lastOrNull() ?: routeRequest.destination,
                    providerId = packageRef.provenance.providerId,
                )
                val sessionSourceMode = if (packageRef.provenance.providerId == RouteProviderId.OSM) RouteSourceMode.OSM else currentSourceMode
                applySelectedAlternativeToSession(sessionSourceMode, routeRequest.destination, item.title)
                onComplete()
            } else if (item.destination != null) {
                routeRequest = RoutePlanRequest(
                    origin = simulatedRiderLocation,
                    destination = item.destination,
                    providerId = currentSourceMode.primaryProviderId,
                )
                planRoute(currentSourceMode, item.title, onComplete = onComplete)
            } else {
                onComplete()
            }
        }
    }

    fun rerouteActiveSession(riderLocation: CoordinatePoint, reason: String) {
        val destination = activeSession.destinationCoordinate ?: return
        viewModelScope.launch {
            val routeIdentifier = activeSession.routeIdentifier ?: preview.routeIdentifier ?: "preview-route"
            bleService.receiveRerouteRequest(
                RouteRerouteRequestMessage(
                    routeIdentifier = routeIdentifier,
                    riderLocation = riderLocation,
                    reason = reason,
                ),
            )
            simulatedRiderLocation = riderLocation
            routeRequest = RoutePlanRequest(
                origin = riderLocation,
                destination = destination,
                providerId = activeSession.sourceMode.primaryProviderId,
            )
            preview = buildPreview(routeRequest, activeSession.sourceMode, (activeSession.routeRevision ?: 0) + 1)
            applySelectedAlternativeToSession(activeSession.sourceMode, destination, activeSession.destinationLabel)
            activeSession = activeSession.copy(
                lastRerouteReason = reason,
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

    private suspend fun buildPreview(request: RoutePlanRequest, sourceMode: RouteSourceMode, revisionOverride: Int?): RoutePreviewModel {
        return when (sourceMode) {
            RouteSourceMode.MIXED -> buildMixedPreview(request, revisionOverride)
            RouteSourceMode.HSL, RouteSourceMode.OSM -> {
                val provider = providers[sourceMode.primaryProviderId] ?: error("Missing provider for ${sourceMode.displayName}")
                val providerPreview = previewFrom(provider, request, revisionOverride)
                val alternatives = presentAlternatives(providerPreview.alternatives)
                providerPreview.copy(
                    alternatives = alternatives,
                    selectedAlternativeId = alternatives.firstOrNull()?.id,
                    routeIdentifier = alternatives.firstOrNull()?.normalizedPackage?.routeIdentifier,
                    routeRevision = alternatives.firstOrNull()?.normalizedPackage?.revision,
                )
            }
        }
    }

    private suspend fun previewFrom(provider: RoutingProvider, request: RoutePlanRequest, revisionOverride: Int?): RoutePreviewModel {
        return if (revisionOverride != null) {
            provider.replanRoute(
                activeSession.copy(
                    routeRevision = revisionOverride - 1,
                    destinationCoordinate = request.destination,
                    providerId = provider.providerId,
                    sourceMode = currentSourceMode,
                ),
                request.origin,
            )
        } else {
            provider.planRoute(request)
        }
    }

    private suspend fun buildMixedPreview(request: RoutePlanRequest, revisionOverride: Int?): RoutePreviewModel {
        val hsl = providers[RouteProviderId.HSL] ?: error("Missing HSL provider")
        val osm = providers[RouteProviderId.OSM] ?: error("Missing OSM provider")
        val previews = listOf(
            previewFrom(hsl, request, revisionOverride),
            previewFrom(osm, request, revisionOverride),
        )
        val effectivePreviews = preferredMixedPreviews(previews)
        val mixedAlternatives = mergeMixedAlternatives(effectivePreviews.flatMap { it.alternatives })
        return RoutePreviewModel(
            alternatives = mixedAlternatives,
            selectedAlternativeId = mixedAlternatives.firstOrNull()?.id,
            routeIdentifier = mixedAlternatives.firstOrNull()?.normalizedPackage?.routeIdentifier,
            routeRevision = mixedAlternatives.firstOrNull()?.normalizedPackage?.revision,
            planningNotice = mixedPlanningNotice(previews, effectivePreviews),
        )
    }

    private fun preferredMixedPreviews(previews: List<RoutePreviewModel>): List<RoutePreviewModel> {
        val livePreviews = previews.filter { !isSamplePreview(it) && it.alternatives.isNotEmpty() }
        return if (livePreviews.isNotEmpty()) livePreviews else previews.filter { it.alternatives.isNotEmpty() }
    }

    private fun isSamplePreview(preview: RoutePreviewModel): Boolean {
        return preview.planningNotice?.contains("sample", ignoreCase = true) == true
    }

    private fun mixedPlanningNotice(previews: List<RoutePreviewModel>, effectivePreviews: List<RoutePreviewModel>): String {
        if (effectivePreviews.size == 1) {
            effectivePreviews.first().planningNotice?.takeIf { it.isNotBlank() }?.let { return it }
        }
        if (effectivePreviews.size < previews.size) {
            return "Showing live routes while sample fallback providers are hidden."
        }
        return "Mixed routes from HSL and OSM"
    }

    private fun mergeMixedAlternatives(alternatives: List<RouteAlternative>): List<RouteAlternative> {
        if (alternatives.isEmpty()) return emptyList()
        val remaining = alternatives.sortedWith(compareBy<RouteAlternative> { it.durationSeconds }.thenBy { it.distanceMeters }).toMutableList()
        val chosen = mutableListOf<RouteAlternative>()

        remaining.removeFirstOrNull()?.let { fastest ->
            chosen += fastest
            remaining.removeAll { it.normalizedPackage.routeIdentifier == fastest.normalizedPackage.routeIdentifier }
        }
        (remaining.firstOrNull { it.normalizedPackage.provenance.providerId == RouteProviderId.OSM } ?: remaining.firstOrNull())?.let { quieter ->
            chosen += quieter
            remaining.removeAll { it.normalizedPackage.routeIdentifier == quieter.normalizedPackage.routeIdentifier }
        }
        remaining.minByOrNull { it.normalizedPackage.maneuverCount }?.let { simpler ->
            chosen += simpler
            remaining.removeAll { it.normalizedPackage.routeIdentifier == simpler.normalizedPackage.routeIdentifier }
        }
        while (chosen.size < 3 && remaining.isNotEmpty()) {
            chosen += remaining.removeAt(0)
        }
        return presentAlternatives(chosen)
    }

    private fun presentAlternatives(alternatives: List<RouteAlternative>): List<RouteAlternative> {
        val labels = listOf(RouteSuggestionKind.FASTEST, RouteSuggestionKind.QUIETER, RouteSuggestionKind.SIMPLER)
        return alternatives.take(3).mapIndexed { index, alternative ->
            alternative.copy(
                title = labels[minOf(index, labels.lastIndex)].displayName,
                subtitle = "via ${alternative.normalizedPackage.provenance.providerId.displayName}",
            )
        }
    }

    private fun applySelectedAlternativeToSession(sourceMode: RouteSourceMode, destination: CoordinatePoint, preferredTitle: String?) {
        val selectedPackage = preview.selectedAlternative?.normalizedPackage
        val providerId = selectedPackage?.provenance?.providerId ?: sourceMode.primaryProviderId
        selectedProviderId = providerId
        activeSession = activeSession.copy(
            routeIdentifier = selectedPackage?.routeIdentifier ?: preview.routeIdentifier,
            routeRevision = selectedPackage?.revision ?: preview.routeRevision,
            destinationLabel = selectedPackage?.summary?.destinationLabel ?: preferredTitle ?: "${providerId.displayName} route",
            destinationCoordinate = selectedPackage?.geometry?.lastOrNull() ?: destination,
            providerId = providerId,
            sourceMode = sourceMode,
        )
    }
}
