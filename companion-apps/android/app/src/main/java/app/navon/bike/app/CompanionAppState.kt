package app.navon.bike.app

import android.app.Application
import android.content.Context
import android.content.Intent
import android.net.Uri
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import androidx.lifecycle.AndroidViewModel
import androidx.lifecycle.viewModelScope
import kotlinx.coroutines.flow.collectLatest
import kotlinx.coroutines.async
import kotlinx.coroutines.coroutineScope
import kotlinx.coroutines.launch
import app.navon.bike.domain.ActiveRouteSession
import app.navon.bike.domain.CompanionSettings
import app.navon.bike.domain.CoordinatePoint
import app.navon.bike.domain.DeviceConnectionState
import app.navon.bike.domain.ImportDiagnosticsEntry
import app.navon.bike.domain.SharedImportClassification
import app.navon.bike.domain.SharedImportDisposition
import app.navon.bike.domain.SharedImportEnvelope
import app.navon.bike.domain.NormalizedRoutePackage
import app.navon.bike.domain.PairedDeviceType
import app.navon.bike.domain.PairedPeripheralRecord
import app.navon.bike.domain.PairingFlowState
import app.navon.bike.domain.RouteAlternative
import app.navon.bike.domain.RouteHistoryItem
import app.navon.bike.domain.RouteHistorySource
import app.navon.bike.domain.RoutePlanRequest
import app.navon.bike.domain.RoutePlannerPreferences
import app.navon.bike.domain.RouteProviderId
import app.navon.bike.domain.RouteRerouteRequestMessage
import app.navon.bike.domain.RerouteContext
import app.navon.bike.domain.RoutingDiagEventData
import app.navon.bike.domain.RoutingDiagSession
import app.navon.bike.domain.RouteAltInfo
import app.navon.bike.domain.LOCATION_EVENT_THROTTLE_MS
import app.navon.bike.domain.RoutePreviewModel
import app.navon.bike.domain.RouteSourceMode
import app.navon.bike.domain.RoutingProvider
import app.navon.bike.domain.SyncSessionState
import app.navon.bike.domain.LocationService
import app.navon.bike.domain.geo.FinlandBounds
import app.navon.bike.integration.AndroidLocationService
import app.navon.bike.feature.device.PhoneGpsForwarder
import app.navon.bike.integration.ble.BleRouteSyncService
import app.navon.bike.integration.ble.PairingQrPayload
import app.navon.bike.integration.ble.navdevice.BeelineDeviceManager
import app.navon.bike.integration.ble.navdevice.BeelineSessionState
import app.navon.bike.integration.diagnostics.CompanionDiagnosticsStore
import app.navon.bike.integration.diagnostics.RoutingDiagnosticsStore
import app.navon.bike.integration.cycling.OsmCyclingRoutingAdapter
import app.navon.bike.integration.gpx.GpxRoutingAdapter
import app.navon.bike.integration.hsl.HslRoutingAdapter
import app.navon.bike.integration.persistence.CompanionPersistence
import app.navon.bike.integration.sample.SampleRoutingAdapter
import app.navon.bike.integration.share.AndroidShareImportParser

enum class GpsSourceSelection {
    INTERNAL,
    PHONE,
}

class CompanionAppState(
    application: Application,
    persistenceOverride: CompanionPersistence? = null,
    bleServiceOverride: BleRouteSyncService? = null,
    locationServiceOverride: LocationService? = null,
    beelineManagerOverride: BeelineDeviceManager? = null,
) : AndroidViewModel(application) {
    companion object {
        private val DEFAULT_RIDER_FALLBACK = CoordinatePoint(60.1699, 24.9384)

        /**
         * iOS-parity helper. Maps the route alternative's provider +
         * sourceReference onto the short engine-derived title shown in
         * the suggested-routes card, dropping the per-provider counter
         * and the redundant "via …" subtitle:
         *
         *   - OSM via BRouter `fastbike` → "BRouter fastbike"
         *   - OSM via BRouter `trekking` → "BRouter trekking"
         *   - OSM via OSRM bike          → "OSM Route"
         *   - HSL Digitransit live / fastest     → "HSL Fastest"
         *   - HSL Digitransit live / alternative → "HSL Route"
         */
        fun friendlyAlternativeLabel(alternative: RouteAlternative): Pair<String, String> {
            val providerId = alternative.normalizedPackage.provenance.providerId
            val sourceRef = alternative.normalizedPackage.provenance.sourceReference?.lowercase().orEmpty()
            return when (providerId) {
                RouteProviderId.OSM -> when {
                    sourceRef.contains("fastbike") -> "BRouter fastbike" to ""
                    sourceRef.contains("trekking") -> "BRouter trekking" to ""
                    else -> "OSM Route" to ""
                }
                RouteProviderId.HSL -> when {
                    sourceRef.contains("fastest") -> "HSL Fastest" to ""
                    else -> "HSL Route" to ""
                }
                RouteProviderId.GPX_IMPORT,
                RouteProviderId.FIT_IMPORT,
                RouteProviderId.TCX_IMPORT -> providerId.displayName to ""
            }
        }
    }

    var selectedProviderId by mutableStateOf(RouteProviderId.HSL)
    var currentSourceMode by mutableStateOf(RouteSourceMode.MIXED)
    var settings by mutableStateOf(CompanionSettings())
    var routePlannerPreferences by mutableStateOf(RoutePlannerPreferences())
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
    var shareImportEventId by mutableStateOf(0L)
    private var persistenceRevision by mutableIntStateOf(0)

    val diagnosticsStore = CompanionDiagnosticsStore()
    val persistence: CompanionPersistence =
        persistenceOverride ?: CompanionPersistence(application.applicationContext)
    val routingDiagnosticsStore: RoutingDiagnosticsStore by lazy {
        RoutingDiagnosticsStore(persistence)
    }
    val bleService: BleRouteSyncService =
        bleServiceOverride ?: BleRouteSyncService(application.applicationContext)
    val locationService: LocationService =
        locationServiceOverride ?: AndroidLocationService(application.applicationContext, persistence)

    /**
     * Beeline handlebar-device transport. The ESP32 route-sync path ([bleService])
     * and this run side-by-side, but only the one matching the paired record's
     * [PairedDeviceType] is fed a route at a time (see [activeDeviceIsBeeline]).
     */
    val beelineManager: BeelineDeviceManager =
        beelineManagerOverride ?: BeelineDeviceManager(
            application.applicationContext,
            planningSpeedKph = settings.cyclingSpeedKph,
        )

    /** Latest Beeline session snapshot, mirrored from [BeelineDeviceManager.state]. */
    var beelineSession by mutableStateOf(BeelineSessionState())
        private set

    var locationState by mutableStateOf(locationService.state.value)

    /**
     * Currently bonded BLE peripheral, loaded from persistence on init.
     * Drives the home-screen device chip and the fast-path reconnect.
     */
    var pairedPeripheral by mutableStateOf<PairedPeripheralRecord?>(persistence.loadPairedPeripheral())
        private set

    /** Tracks the QR-OOB pairing flow's current step. */
    var pairingState by mutableStateOf<PairingFlowState>(PairingFlowState.Idle)
        private set

    /** GPS source selection for the device settings screen. */
    var gpsSource by mutableStateOf(GpsSourceSelection.INTERNAL)

    /** Whether the phone GPS forwarder is actively sending data. */
    var isPhoneGpsForwarding by mutableStateOf(false)
        private set

    /** Phone GPS forwarding coroutine-backed writer. */
    private val phoneGpsForwarder: PhoneGpsForwarder by lazy {
        PhoneGpsForwarder(
            bleClient = bleService.bluetoothClient,
            locationService = locationService as AndroidLocationService,
        )
    }

    /** Best estimate of where the rider currently is. Falls back to last-known then default. */
    val riderLocation: CoordinatePoint
        get() = locationState.currentLocation
            ?: locationState.lastKnownLocation
            ?: DEFAULT_RIDER_FALLBACK

    /** True when we have launched a watcher but no usable fix has arrived yet. */
    val isWaitingForFirstLocationFix: Boolean
        get() = locationState.isLocating
            && locationState.currentLocation == null
            && locationState.lastKnownLocation == null

    private var lastHandledRerouteSignature: String? = null

    private val providers: Map<RouteProviderId, RoutingProvider> = mapOf(
        RouteProviderId.HSL to HslRoutingAdapter(settingsProvider = { settings }),
        RouteProviderId.OSM to OsmCyclingRoutingAdapter(),
        RouteProviderId.GPX_IMPORT to GpxRoutingAdapter(),
        RouteProviderId.FIT_IMPORT to SampleRoutingAdapter(RouteProviderId.FIT_IMPORT),
        RouteProviderId.TCX_IMPORT to SampleRoutingAdapter(RouteProviderId.TCX_IMPORT),
    )

    /**
     * True when both endpoints of the current request fall inside Finland
     * (Digitransit's nationwide coverage area).
     */
    val isHslApplicableForRequest: Boolean
        get() = isInFinland(routeRequest.origin) && isInFinland(routeRequest.destination)

    /** True when HSL is geographically usable for the current request. */
    val isHslAvailable: Boolean
        get() = isHslApplicableForRequest

    /**
     * Source-mode tabs visible in the UI. When either endpoint is outside Finland,
     * mixed/HSL collapse to OSM (the picker hides itself when there is only one option).
     */
    val sourceModeOptions: List<RouteSourceMode>
        get() = if (isHslAvailable) RouteSourceMode.entries else listOf(RouteSourceMode.OSM)

    private fun isInFinland(point: CoordinatePoint): Boolean = FinlandBounds.contains(point)

    val routeHistoryItems: List<RouteHistoryItem>
        get() {
            persistenceRevision
            return persistence.loadRecentRouteHistory()
        }

    val importDiagnosticsEntries: List<ImportDiagnosticsEntry>
        get() {
            persistenceRevision
            return persistence.loadImportDiagnostics()
        }

    val isDeviceConnected: Boolean
        get() = syncSession.connectionState == app.navon.bike.domain.DeviceConnectionState.CONNECTED ||
            beelineSession.connectionState == app.navon.bike.domain.DeviceConnectionState.CONNECTED

    /** True when the paired device is a Beeline (route + GPS go to [beelineManager], not [bleService]). */
    val activeDeviceIsBeeline: Boolean
        get() = pairedPeripheral?.effectiveDeviceType == PairedDeviceType.BEELINE

    /**
     * Connection state of whichever transport backs the paired device, so the
     * home chip and device settings render one device through one vocabulary
     * regardless of whether it's the ESP32 or a Beeline.
     */
    val deviceConnectionState: DeviceConnectionState
        get() = if (activeDeviceIsBeeline) beelineSession.connectionState else syncSession.connectionState

    init {
        settings = persistence.loadSettings()
        routePlannerPreferences = persistence.loadRoutePlannerPreferences()
        currentSourceMode = routePlannerPreferences.defaultSourceMode
        persistence.loadLastSession()?.let { activeSession = it }
        selectedProviderId = activeSession.providerId
        // Seed the route request origin from the last persisted fix if we have one.
        locationState.lastKnownLocation?.let { routeRequest = routeRequest.copy(origin = it) }
        viewModelScope.launch {
            var lastRecordedLocationMs = 0L
            locationService.state.collectLatest { state ->
                locationState = state
                state.currentLocation?.let { fix ->
                    if (routeRequest.origin != fix) routeRequest = routeRequest.copy(origin = fix)
                    // Feed live fixes to a connected Beeline so it can advance
                    // its turn-by-turn frame; the ESP32 path forwards GPS via
                    // PhoneGpsForwarder instead.
                    if (activeDeviceIsBeeline && beelineManager.isConnected) {
                        beelineManager.onLocation(fix, state.currentSpeedMps)
                    }
                    if (routingDiagnosticsStore.isRecording) {
                        val now = System.currentTimeMillis()
                        if (now - lastRecordedLocationMs >= LOCATION_EVENT_THROTTLE_MS) {
                            lastRecordedLocationMs = now
                            routingDiagnosticsStore.recordEvent(
                                RoutingDiagEventData.locationUpdate(
                                    lat = fix.latitude,
                                    lon = fix.longitude,
                                    heading = null,
                                    speed = state.currentSpeedMps,
                                )
                            )
                        }
                    }
                }
            }
        }
        viewModelScope.launch {
            bleService.state.collectLatest {
                syncSession = it
                refreshDiagnostics()
                val inbound = it.lastInboundMessage
                if (inbound is app.navon.bike.domain.RouteSyncMessage.RerouteRequest) {
                    val message = inbound.message
                    val signature = "${message.routeIdentifier}-${message.riderLocation.latitude}-${message.riderLocation.longitude}-${message.reason}"
                    if (signature != lastHandledRerouteSignature) {
                        lastHandledRerouteSignature = signature
                        rerouteActiveSession(message.riderLocation, message.reason)
                    }
                }
            }
        }

        viewModelScope.launch {
            beelineManager.state.collectLatest { beelineSession = it }
        }

        // App-launch auto-reconnect: if the user already paired a
        // device, try to reach it once silently. Failures are silent —
        // the home chip stays in `PairedDisconnected` and the user can
        // tap it to retry. We don't fall back to a service-UUID scan
        // here so a stranger's device doesn't silently take the
        // paired slot; that path stays gated behind explicit user
        // action via `connectToDevice()`.
        pairedPeripheral?.let { record ->
            viewModelScope.launch {
                runCatching {
                    if (record.effectiveDeviceType == PairedDeviceType.BEELINE) {
                        beelineManager.connect(record.identifier)
                    } else {
                        bleService.connectToPairedPeripheral(record.identifier)
                    }
                }
                refreshDiagnostics()
            }
        }
    }

    /** Begin watching the device location. Call from MainActivity once permission is granted. */
    fun startLocationUpdates() {
        locationService.start()
    }

    /** Stop watching the device location (called on background or stop). */
    fun stopLocationUpdates() {
        locationService.stop()
    }

    override fun onCleared() {
        super.onCleared()
        locationService.stop()
        beelineManager.stopNavigation()
    }

    fun persistSettings() {
        persistence.saveSettings(settings)
        normalizeSourceModeForHslAvailability()
    }

    /**
     * When HSL becomes unusable (no key OR endpoints outside Finland), fall back any
     * HSL-only or Mixed active selections to OSM. Persisted defaults are also normalised
     * when the underlying *configuration* (the key) is gone, so a relaunch is consistent.
     */
    fun normalizeSourceModeForHslAvailability() {
        if (!isHslAvailable && currentSourceMode != RouteSourceMode.OSM) {
            currentSourceMode = RouteSourceMode.OSM
        }
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

    fun planRoute(
        sourceMode: RouteSourceMode = currentSourceMode,
        preferredTitle: String? = null,
        revisionOverride: Int? = null,
        rerouteContext: RerouteContext? = null,
        onComplete: () -> Unit = {},
    ) {
        // Collapse mixed/HSL down to OSM when HSL isn't available for this trip
        // (no key OR endpoints outside Finland).
        val effectiveMode = if (!isHslAvailable && sourceMode != RouteSourceMode.OSM) RouteSourceMode.OSM else sourceMode
        currentSourceMode = effectiveMode
        routeRequest = routeRequest.copy(providerId = effectiveMode.primaryProviderId)
        viewModelScope.launch {
            runCatching {
                val nextPreview = buildPreview(routeRequest, effectiveMode, revisionOverride, rerouteContext)
                preview = nextPreview
                if (routingDiagnosticsStore.isRecording) {
                    val alts = nextPreview.alternatives.map { alt ->
                        RouteAltInfo(
                            providerName = alt.normalizedPackage.provenance.providerId.name,
                            routeId = alt.normalizedPackage.routeIdentifier,
                            label = alt.title,
                        )
                    }
                    routingDiagnosticsStore.recordEvent(RoutingDiagEventData.routeAlternativesSuggested(alts))
                }
                persistence.saveRecentDestination(routeRequest.destination)
                applySelectedAlternativeToSession(effectiveMode, routeRequest.destination, preferredTitle)
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
                if (activeDeviceIsBeeline) {
                    // Beeline drives turn-by-turn off the full route package +
                    // live GPS rather than chunked route-sync. Ensure connected,
                    // then (re)start navigation along the new route.
                    if (!beelineManager.isConnected) {
                        pairedPeripheral?.let { beelineManager.connect(it.identifier) }
                    }
                    beelineManager.startNavigation(normalized, riderLocation, locationState.currentSpeedMps)
                } else {
                    val shouldUpdate = syncSession.activeRouteIdentifier == normalized.routeIdentifier && syncSession.activeRouteRevision != null
                    if (shouldUpdate) {
                        bleService.publishUpdate(normalized)
                    } else {
                        bleService.publishSet(normalized)
                    }
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
                if (activeDeviceIsBeeline) {
                    beelineManager.stopNavigation()
                } else {
                    bleService.publishClear(activeSession.routeIdentifier)
                }
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

    /** Update preview selection without touching activeSession.destinationLabel.
     *  Used during alternatives exploration so the rider's typed destination is preserved. */
    fun selectAlternativePreviewOnly(alternativeId: String) {
        preview = preview.copy(
            selectedAlternativeId = alternativeId,
            routeIdentifier = preview.alternatives.firstOrNull { it.id == alternativeId }?.normalizedPackage?.routeIdentifier,
            routeRevision = preview.alternatives.firstOrNull { it.id == alternativeId }?.normalizedPackage?.revision,
        )
        preview.selectedAlternative?.normalizedPackage?.provenance?.providerId?.let { selectedProviderId = it }
        refreshDiagnostics()
    }

    fun connectToDevice() {
        viewModelScope.launch {
            val paired = pairedPeripheral
            // Beeline reconnects by MAC over its own transport — no
            // service-UUID scan fallback (that path is ESP32-only).
            if (paired != null && paired.effectiveDeviceType == PairedDeviceType.BEELINE) {
                runCatching { beelineManager.connect(paired.identifier) }
                refreshDiagnostics()
                return@launch
            }
            // Fast path when we already know the peripheral identifier:
            // skip scanning entirely. Only fall back to a scan when we
            // either have no record or the identifier failed to connect
            // (the pairing flow + iOS-equivalent both follow this rule).
            if (paired != null) {
                bleService.connectToPairedPeripheral(paired.identifier)
            }
            if (bleService.state.value.connectionState != DeviceConnectionState.CONNECTED) {
                bleService.scanForDevices()
                bleService.connectToLastKnownDevice()
            }
            refreshDiagnostics()
        }
    }

    /**
     * Pair a Beeline Velo2/Moto2. Unlike the ESP32 QR-OOB flow, the Beeline is
     * discovered by a BLE name scan and bonds on first connect, so there is no
     * camera step. On success the bonded record is persisted with
     * [PairedDeviceType.BEELINE] and becomes the active device.
     *
     * @param onComplete invoked with `true` on success, `false` on failure;
     *   the failure reason is surfaced via [beelineSession]'s `lastError`.
     */
    fun pairBeelineDevice(onComplete: (Boolean) -> Unit = {}) {
        pairingState = PairingFlowState.Scanning
        viewModelScope.launch {
            val result = beelineManager.pair()
            result.onSuccess { scanned ->
                val record = PairedPeripheralRecord(
                    identifier = scanned.address,
                    friendlyName = scanned.name ?: "Beeline",
                    pairedAt = java.time.Instant.now().toString(),
                    deviceType = PairedDeviceType.BEELINE,
                )
                persistence.savePairedPeripheral(record)
                pairedPeripheral = record
                pairingState = PairingFlowState.Succeeded
                refreshDiagnostics()
                onComplete(true)
            }.onFailure { error ->
                pairingState = PairingFlowState.Failed(
                    error.localizedMessage ?: "Couldn't pair the Beeline",
                )
                onComplete(false)
            }
        }
    }

    /**
     * Enter the pairing flow's introductory step. The UI sheet observes
     * `pairingState` and walks the user through QR scan → confirm.
     */
    fun handleGpsSourceChange(source: GpsSourceSelection) {
        when (source) {
            GpsSourceSelection.INTERNAL -> phoneGpsForwarder.stop()
            GpsSourceSelection.PHONE -> phoneGpsForwarder.start()
        }
    }

    fun beginPairingFlow() {
        pairingState = PairingFlowState.Instructions
    }

    /** Close the pairing sheet (cancel / done). Resets the flow to [PairingFlowState.Idle]. */
    fun dismissPairingFlow() {
        pairingState = PairingFlowState.Idle
    }

    /**
     * Disconnect the active device without forgetting the bond. Implemented
     * for Beeline today; the ESP32 route-sync service has no explicit
     * disconnect yet (it tears down with the GATT connection), so this is a
     * no-op for that device type.
     */
    fun disconnectDevice() {
        if (activeDeviceIsBeeline) {
            beelineManager.disconnect()
            refreshDiagnostics()
        }
    }

    /**
     * Step the device into pairing mode before opening the camera.
     *
     * The device defaults to showing the map; the QR overlay is only
     * rendered after a companion writes `pairing_request` over BLE.
     * The pairing flow runs this *between* the instructions step and
     * the camera step so the device's panel actually shows the QR by
     * the time the user holds the phone up to it.
     *
     * Steps:
     *   1. service-UUID scan to locate the device (no fast-path here —
     *      the user might be re-pairing after Forget, in which case
     *      `pairedPeripheral` was just cleared).
     *   2. connect to whatever advertised the route-sync service.
     *   3. write `pairing_request` (unencrypted) so the device flips
     *      from map to QR.
     *
     * Throws on any step failure — the pairing-flow view-model
     * surfaces that as a user-visible error before the camera opens.
     */
    suspend fun prepareDeviceForPairing() {
        pairingState = PairingFlowState.Connecting
        bleService.scanForDevices()
        bleService.connectToLastKnownDevice()
        if (bleService.state.value.connectionState != DeviceConnectionState.CONNECTED) {
            val reason = bleService.state.value.lastSyncResult
                ?: "Couldn't connect to the device"
            pairingState = PairingFlowState.Failed(reason)
            error(reason)
        }
        bleService.writePairingRequest()
        pairingState = PairingFlowState.Scanning
    }

    /**
     * Drive the pairing handshake to completion: connect to the
     * advertised peripheral, write the QR's secret to the
     * pairing-confirm characteristic, then persist the bond. On any
     * step failure, no half-state is committed — the persisted record
     * is left untouched so the user can retry.
     *
     * The auto-dismiss delay (1.5s) lets the success state render before
     * we drop back to `Idle`; tests pass `autoDismissDelayMs = 0L` to
     * skip the delay.
     */
    suspend fun completePairing(
        payload: PairingQrPayload,
        autoDismissDelayMs: Long = 1_500L,
    ) {
        // Reuse the existing connection from `prepareDeviceForPairing`
        // when possible — we don't want to scan again, that would
        // require the QR to still be visible on the device. If we
        // somehow got disconnected between prepareDeviceForPairing and
        // here, fall back to the QR's `id_android` for a targeted
        // reconnect.
        val deviceName = if (
            bleService.state.value.connectionState == DeviceConnectionState.CONNECTED
        ) {
            bleService.state.value.lastDeviceName ?: "Navon"
        } else {
            pairingState = PairingFlowState.Connecting
            try {
                bleService.connectToAdvertisedPeripheral(payload.peripheralIdentifier)
            } catch (error: Throwable) {
                pairingState = PairingFlowState.Failed(
                    error.localizedMessage ?: "BLE connection failed during pairing",
                )
                return
            }
        }

        pairingState = PairingFlowState.Confirming
        try {
            bleService.writePairingConfirm(payload.ephemeralSecret)
        } catch (error: Throwable) {
            pairingState = PairingFlowState.Failed(
                error.localizedMessage ?: "Pairing-confirm write failed",
            )
            return
        }

        val record = PairedPeripheralRecord(
            identifier = payload.peripheralIdentifier,
            friendlyName = deviceName,
            pairedAt = java.time.Instant.now().toString(),
        )
        persistence.savePairedPeripheral(record)
        pairedPeripheral = record
        pairingState = PairingFlowState.Succeeded

        if (autoDismissDelayMs > 0L) {
            kotlinx.coroutines.delay(autoDismissDelayMs)
        }
        pairingState = PairingFlowState.Idle
    }

    /**
     * Drop the bond. The next connection attempt will need to go
     * through the full pairing flow again. Mirrors the iOS-side
     * `AppModel.forgetPairedDevice` so the cross-platform UX has parity.
     */
    fun forgetPairedDevice() {
        if (activeDeviceIsBeeline) {
            beelineManager.disconnect()
        }
        persistence.clearPairedPeripheral()
        pairedPeripheral = null
        pairingState = PairingFlowState.Idle
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
        bleService.armFaultInjection(app.navon.bike.domain.RouteSyncFaultInjectionMode.WRITE_FAILURE)
        refreshDiagnostics()
    }

    fun armDisconnectAfterNextChunkWrite() {
        bleService.armFaultInjection(app.navon.bike.domain.RouteSyncFaultInjectionMode.DISCONNECT_AFTER_CHUNK_WRITE)
        refreshDiagnostics()
    }

    fun armDropNextInboundStatus() {
        bleService.armFaultInjection(app.navon.bike.domain.RouteSyncFaultInjectionMode.DROP_NEXT_INBOUND_STATUS)
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
                title = activeSession.destinationLabel,
                subtitle = selected.summaryLine,
                source = source,
                sourceLabel = sourceLabel,
                createdAtLabel = "Just now",
                destination = selected.geometry.lastOrNull(),
                routePackage = selected,
                occurrenceCount = null,
            ),
        )
        notePersistenceChanged()
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
                occurrenceCount = 1,
            ),
        )
        persistence.saveRecentDestination(coordinate)
        notePersistenceChanged()
    }

    fun dismissRouteHistoryItem(id: String) {
        persistence.dismissRouteHistoryItem(id)
        notePersistenceChanged()
    }

    fun dismissImportDiagnosticsEntry(id: String) {
        persistence.dismissImportDiagnosticsEntry(id)
        notePersistenceChanged()
    }

    val routingDiagnosticsSessions: List<RoutingDiagSession>
        get() = routingDiagnosticsStore.sessions.value

    fun dismissRoutingDiagnosticsSession(id: String) {
        routingDiagnosticsStore.deleteSession(id)
        notePersistenceChanged()
    }

    fun handleSharedIntent(intent: Intent?, sourceApplication: String? = null): Boolean {
        return shareImportHandleIntent(this, intent, sourceApplication)
    }

    fun retrySharedImport(entry: ImportDiagnosticsEntry) {
        shareImportRetry(this, entry)
    }

    fun notePersistenceChanged() {
        persistenceRevision += 1
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
                    origin = riderLocation,
                    destination = item.destination ?: packageRef.geometry.lastOrNull() ?: routeRequest.destination,
                    providerId = packageRef.provenance.providerId,
                )
                val sessionSourceMode = if (packageRef.provenance.providerId == RouteProviderId.OSM) RouteSourceMode.OSM else currentSourceMode
                applySelectedAlternativeToSession(sessionSourceMode, routeRequest.destination, item.title)
                onComplete()
            } else if (item.destination != null) {
                routeRequest = RoutePlanRequest(
                    origin = riderLocation,
                    destination = item.destination,
                    providerId = currentSourceMode.primaryProviderId,
                )
                planRoute(currentSourceMode, item.title, onComplete = onComplete)
            } else {
                onComplete()
            }
        }
    }

    fun rerouteActiveSession(
        riderLocation: CoordinatePoint,
        reason: String,
        rerouteContext: RerouteContext? = null,
    ) {
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
            routeRequest = RoutePlanRequest(
                origin = riderLocation,
                destination = destination,
                providerId = activeSession.sourceMode.primaryProviderId,
            )
            preview = buildPreview(
                routeRequest,
                activeSession.sourceMode,
                (activeSession.routeRevision ?: 0) + 1,
                rerouteContext,
            )
            applySelectedAlternativeToSession(activeSession.sourceMode, destination, activeSession.destinationLabel)
            activeSession = activeSession.copy(
                lastRerouteReason = reason,
                lastRerouteTimestamp = "Just now",
            )
            sendSelectedRoute { success ->
                routingDiagnosticsStore.recordEvent(
                    RoutingDiagEventData.rerouteCompleted(if (success) "success" else "failed")
                )
            }
        }
    }

    fun refreshDiagnostics() {
        diagnosticsStore.update(
            session = activeSession.takeIf { it.routeIdentifier != null },
            syncState = syncSession,
        )
    }

    private suspend fun buildPreview(
        request: RoutePlanRequest,
        sourceMode: RouteSourceMode,
        revisionOverride: Int?,
        rerouteContext: RerouteContext? = null,
    ): RoutePreviewModel {
        return when (sourceMode) {
            RouteSourceMode.MIXED -> buildMixedPreview(request, revisionOverride, rerouteContext)
            RouteSourceMode.HSL, RouteSourceMode.OSM -> {
                val provider = providers[sourceMode.primaryProviderId] ?: error("Missing provider for ${sourceMode.displayName}")
                val providerPreview = previewFrom(provider, request, revisionOverride, rerouteContext)
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

    private suspend fun previewFrom(
        provider: RoutingProvider,
        request: RoutePlanRequest,
        revisionOverride: Int?,
        rerouteContext: RerouteContext? = null,
    ): RoutePreviewModel {
        return if (revisionOverride != null) {
            provider.replanRoute(
                activeSession.copy(
                    routeRevision = revisionOverride - 1,
                    destinationCoordinate = request.destination,
                    providerId = provider.providerId,
                    sourceMode = currentSourceMode,
                ),
                request.origin,
                rerouteContext,
            )
        } else {
            provider.planRoute(request)
        }
    }

    private suspend fun buildMixedPreview(
        request: RoutePlanRequest,
        revisionOverride: Int?,
        rerouteContext: RerouteContext? = null,
    ): RoutePreviewModel = coroutineScope {
        val hsl = providers[RouteProviderId.HSL]
        val osm = providers[RouteProviderId.OSM] ?: return@coroutineScope RoutePreviewModel(
            alternatives = emptyList(),
            planningNotice = "Mixed mode providers are unavailable",
        )
        val osmJob = async { runCatching { previewFrom(osm, request, revisionOverride, rerouteContext) } }
        val hslJob = if (hsl != null) async { runCatching { previewFrom(hsl, request, revisionOverride, rerouteContext) } } else null
        val totalRacers = if (hslJob != null) 2 else 1
        val previews = listOfNotNull(
            osmJob.await().getOrNull(),
            hslJob?.await()?.getOrNull(),
        )
        val effectivePreviews = preferredMixedPreviews(previews)
        val mixedAlternatives = mergeMixedAlternatives(effectivePreviews.flatMap { it.alternatives })
        RoutePreviewModel(
            alternatives = mixedAlternatives,
            selectedAlternativeId = mixedAlternatives.firstOrNull()?.id,
            routeIdentifier = mixedAlternatives.firstOrNull()?.normalizedPackage?.routeIdentifier,
            routeRevision = mixedAlternatives.firstOrNull()?.normalizedPackage?.revision,
            planningNotice = mixedPlanningNotice(effectivePreviews, totalRacers),
        )
    }

    private fun preferredMixedPreviews(previews: List<RoutePreviewModel>): List<RoutePreviewModel> {
        return previews.filter { it.alternatives.isNotEmpty() }
    }

    private fun mixedPlanningNotice(effectivePreviews: List<RoutePreviewModel>, totalRacers: Int): String {
        if (effectivePreviews.size == 1) {
            effectivePreviews.first().planningNotice?.takeIf { it.isNotBlank() }?.let { return it }
        }
        if (effectivePreviews.size < totalRacers) {
            return "Showing available routes while some providers are hidden."
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

    /**
     * Label every visible alternative as "<Provider> Route N", where N is
     * a per-provider counter (so OSM Route 1, OSM Route 2, HSL Route 1,
     * …). Replaces the prior "Fastest / Quieter / Simpler" scheme which
     * implied semantics the routing backends don't deliver — the order
     * is just whatever the provider returned.
     */
    private fun presentAlternatives(alternatives: List<RouteAlternative>): List<RouteAlternative> {
        return alternatives.take(3).map { alternative ->
            val label = friendlyAlternativeLabel(alternative)
            alternative.copy(title = label.first, subtitle = label.second)
        }
    }

    private fun applySelectedAlternativeToSession(sourceMode: RouteSourceMode, destination: CoordinatePoint, preferredTitle: String?) {
        val selectedPackage = preview.selectedAlternative?.normalizedPackage
        val providerId = selectedPackage?.provenance?.providerId ?: sourceMode.primaryProviderId
        selectedProviderId = providerId
        activeSession = activeSession.copy(
            routeIdentifier = selectedPackage?.routeIdentifier ?: preview.routeIdentifier,
            routeRevision = selectedPackage?.revision ?: preview.routeRevision,
            destinationLabel = displayDestinationTitle(selectedPackage, preferredTitle, "No destination"),
            destinationCoordinate = selectedPackage?.geometry?.lastOrNull() ?: destination,
            providerId = providerId,
            sourceMode = sourceMode,
        )
    }

    private fun displayDestinationTitle(selectedPackage: NormalizedRoutePackage?, preferredTitle: String?, fallback: String): String {
        val preferred = preferredTitle?.trim().orEmpty()
        if (preferred.isNotEmpty()) {
            return preferred
        }
        val packageTitle = selectedPackage?.summary?.destinationLabel?.trim().orEmpty()
        if (packageTitle.isNotEmpty() && !isGenericDestinationTitle(packageTitle, selectedPackage?.provenance?.providerId)) {
            return packageTitle
        }
        return fallback
    }

    private fun isGenericDestinationTitle(title: String, providerId: RouteProviderId?): Boolean {
        val normalized = title.trim().lowercase()
        if (normalized.isEmpty()) return true
        if (normalized == "selected destination" || normalized == "route" || normalized == "recent destination" || normalized == "dropped pin") {
            return true
        }
        if (providerId != null && normalized == "${providerId.displayName.lowercase()} sample destination") {
            return true
        }
        return false
    }
}
