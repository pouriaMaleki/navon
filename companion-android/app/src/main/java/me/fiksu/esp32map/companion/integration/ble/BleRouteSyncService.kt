package me.fiksu.esp32map.companion.integration.ble

import android.content.Context
import java.util.UUID
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.launch
import me.fiksu.esp32map.companion.domain.DeviceConnectionState
import me.fiksu.esp32map.companion.domain.NormalizedRoutePackage
import me.fiksu.esp32map.companion.domain.RouteClearMessage
import me.fiksu.esp32map.companion.domain.RouteRerouteRequestMessage
import me.fiksu.esp32map.companion.domain.RouteSetMessage
import me.fiksu.esp32map.companion.domain.RouteStatusMessage
import me.fiksu.esp32map.companion.domain.RouteSyncFaultInjectionMode
import me.fiksu.esp32map.companion.domain.RouteSyncMessage
import me.fiksu.esp32map.companion.domain.RouteSyncState
import me.fiksu.esp32map.companion.domain.RouteSyncStatusCode
import me.fiksu.esp32map.companion.domain.RouteSyncTransport
import me.fiksu.esp32map.companion.domain.RouteTransferProgress
import me.fiksu.esp32map.companion.domain.RouteUpdateMessage
import me.fiksu.esp32map.companion.domain.SyncSessionState

class BleRouteSyncService(
    context: Context,
    private val bluetoothClient: RouteSyncBluetoothClient = AndroidBleRouteSyncClient(context),
) : RouteSyncTransport {
    private val mutableState = MutableStateFlow(SyncSessionState())
    val state: StateFlow<SyncSessionState> = mutableState.asStateFlow()

    private val serviceScope = CoroutineScope(SupervisorJob() + Dispatchers.Main.immediate)
    private var pendingTransfer: PendingTransfer? = null
    private var ackTimeoutJob: Job? = null

    init {
        bluetoothClient.onConnectionStateChange = { state, name ->
            handleConnectionStateChange(state, name)
        }
        bluetoothClient.onSyncMessage = { message ->
            serviceScope.launch {
                handleInbound(message)
            }
        }
    }

    override suspend fun scanForDevices() {
        updateState { it.copy(connectionState = DeviceConnectionState.SCANNING) }
        runCatching { bluetoothClient.scanForRouteSyncPeripheral() }
            .onSuccess { deviceName ->
                updateState {
                    it.copy(
                        lastDeviceName = deviceName,
                        lastSyncResult = "Discovered $deviceName",
                    )
                }
            }
            .onFailure { error ->
                updateState {
                    it.copy(
                        connectionState = DeviceConnectionState.DISCONNECTED,
                        lastSyncResult = error.localizedMessage ?: "BLE scan failed",
                    )
                }
            }
    }

    /**
     * Fast-path reconnect to a previously-bonded peripheral. Skips
     * scanning entirely; on failure the caller is expected to fall back
     * to [scanForDevices] + [connectToLastKnownDevice]. Mirrors the
     * iOS-side `connectToPairedPeripheral`.
     */
    suspend fun connectToPairedPeripheral(identifier: String) {
        updateState { it.copy(connectionState = DeviceConnectionState.CONNECTING) }
        runCatching { bluetoothClient.connectToPairedPeripheral(identifier) }
            .onSuccess { deviceName ->
                updateState {
                    it.copy(
                        connectionState = DeviceConnectionState.CONNECTED,
                        lastDeviceName = deviceName,
                        lastSyncResult = "Reconnected to $deviceName",
                    )
                }
            }
            .onFailure { error ->
                updateState {
                    it.copy(
                        connectionState = DeviceConnectionState.DISCONNECTED,
                        lastSyncResult = error.localizedMessage ?: "BLE fast-path connection failed",
                    )
                }
            }
    }

    /**
     * Connect to an as-yet-unbonded peripheral whose BD_ADDR was just
     * pulled from the pairing-flow QR. Returns the friendly name of the
     * connected peer; throws on failure so the caller can surface a
     * specific error in the pairing UI.
     */
    suspend fun connectToAdvertisedPeripheral(identifier: String): String {
        updateState { it.copy(connectionState = DeviceConnectionState.CONNECTING) }
        return runCatching { bluetoothClient.connectToAdvertisedPeripheral(identifier) }
            .onSuccess { deviceName ->
                updateState {
                    it.copy(
                        connectionState = DeviceConnectionState.CONNECTED,
                        lastDeviceName = deviceName,
                        lastSyncResult = "Connecting for pairing: $deviceName",
                    )
                }
            }
            .onFailure { error ->
                updateState {
                    it.copy(
                        connectionState = DeviceConnectionState.DISCONNECTED,
                        lastSyncResult = error.localizedMessage
                            ?: "BLE pairing connection failed",
                    )
                }
            }
            .getOrThrow()
    }

    /**
     * Write the QR's 32-byte secret to the firmware's pairing-confirm
     * characteristic to close the OOB handshake. Throws on write
     * failure so the pairing UI can surface a retry button.
     */
    suspend fun writePairingConfirm(secret: ByteArray) {
        bluetoothClient.writePairingConfirm(secret)
    }

    override suspend fun connectToLastKnownDevice() {
        updateState { it.copy(connectionState = DeviceConnectionState.CONNECTING) }
        runCatching { bluetoothClient.connectToScannedPeripheral() }
            .onSuccess { deviceName ->
                updateState {
                    it.copy(
                        connectionState = DeviceConnectionState.CONNECTED,
                        lastDeviceName = deviceName,
                        lastSyncResult = "Connected to $deviceName",
                    )
                }
            }
            .onFailure { error ->
                updateState {
                    it.copy(
                        connectionState = DeviceConnectionState.DISCONNECTED,
                        lastSyncResult = error.localizedMessage ?: "BLE connection failed",
                    )
                }
            }
    }

    override suspend fun publishSet(route: NormalizedRoutePackage) {
        beginTransfer(RouteSyncMessage.Set(RouteSetMessage(route)))
    }

    override suspend fun publishUpdate(route: NormalizedRoutePackage) {
        beginTransfer(
            RouteSyncMessage.Update(
                RouteUpdateMessage(
                    routeIdentifier = route.routeIdentifier,
                    revision = route.revision,
                    route = route,
                ),
            ),
        )
    }

    override suspend fun publishClear(routeIdentifier: String?) {
        beginTransfer(RouteSyncMessage.Clear(RouteClearMessage(routeIdentifier)))
    }

    override suspend fun resumePendingTransfer() {
        val transfer = pendingTransfer ?: error("No pending transfer to resume")
        if (transfer.nextChunkIndex >= transfer.chunkEnvelopes.size) {
            val restartedTransfer = transfer.copy(nextChunkIndex = 0)
            pendingTransfer = restartedTransfer
            updateState { it.copy(transferProgress = restartedTransfer.toProgress()) }
        }
        updateState {
            it.copy(
                routeSyncState = RouteSyncState.TRANSFERRING,
                lastSyncResult = "Resuming ${transfer.message.kindLabel} at chunk ${transfer.nextChunkIndex + 1}/${transfer.chunkEnvelopes.size}",
            )
        }
        drainPendingTransfer()
    }

    override fun armRetryableInterruptionOnNextTransfer() {
        armFaultInjection(RouteSyncFaultInjectionMode.RETRYABLE_INTERRUPTION)
    }

    override fun armFaultInjection(mode: RouteSyncFaultInjectionMode) {
        when (mode) {
            RouteSyncFaultInjectionMode.RETRYABLE_INTERRUPTION -> {
                updateState {
                    it.copy(
                        retryableInterruptionArmed = true,
                        armedFaultInjectionMode = mode,
                        lastSyncResult = "Armed ${mode.displayName.lowercase()} for the next BLE sync cycle",
                    )
                }
            }
            RouteSyncFaultInjectionMode.WRITE_FAILURE,
            RouteSyncFaultInjectionMode.DISCONNECT_AFTER_CHUNK_WRITE,
            RouteSyncFaultInjectionMode.DROP_NEXT_INBOUND_STATUS,
            -> {
                bluetoothClient.armDebugFault(mode)
                updateState {
                    it.copy(
                        retryableInterruptionArmed = false,
                        armedFaultInjectionMode = mode,
                        lastSyncResult = "Armed ${mode.displayName.lowercase()} for the next BLE sync cycle",
                    )
                }
            }
        }
    }

    override suspend fun receiveStatus(message: RouteStatusMessage) {
        handleInbound(RouteSyncMessage.Status(message))
    }

    override suspend fun receiveRerouteRequest(message: RouteRerouteRequestMessage) {
        handleInbound(RouteSyncMessage.RerouteRequest(message))
    }

    private suspend fun beginTransfer(message: RouteSyncMessage) {
        cancelAckTimeout()
        val payload = BleRouteSyncCodec.canonicalPayloadBytes(message)
        val transferIdentifier = UUID.randomUUID().toString()
        val transfer = PendingTransfer(
            identifier = transferIdentifier,
            message = message,
            payload = payload,
            checksumHex = BleRouteSyncCodec.checksumHex(payload),
            chunkEnvelopes = BleRouteSyncCodec.chunkEnvelopes(
                message = message,
                transferIdentifier = transferIdentifier,
                chunkSizeBytes = CHUNK_SIZE_BYTES,
            ),
            nextChunkIndex = 0,
            retryCount = 0,
            lastError = null,
            usingLiveTransport = bluetoothClient.isReady,
        )
        pendingTransfer = transfer
        updateState {
            it.copy(
                routeSyncState = RouteSyncState.PREPARING,
                pendingRouteIdentifier = transfer.routeIdentifier,
                pendingRouteRevision = transfer.routeRevision,
                lastOutboundMessage = message,
                transferProgress = transfer.toProgress(),
                lastSyncResult = "Prepared ${message.kindLabel} payload (${payload.size} B across ${transfer.chunkEnvelopes.size} chunks)",
            )
        }
        drainPendingTransfer()
    }

    private suspend fun drainPendingTransfer() {
        var transfer = pendingTransfer ?: error("No pending transfer")
        cancelAckTimeout()
        updateState { it.copy(routeSyncState = RouteSyncState.TRANSFERRING) }

        while (transfer.nextChunkIndex < transfer.chunkEnvelopes.size) {
            delay(80)
            val chunkNumber = transfer.nextChunkIndex + 1
            if (mutableState.value.retryableInterruptionArmed) {
                transfer = transfer.copy(
                    retryCount = transfer.retryCount + 1,
                    lastError = "Simulated BLE interruption at chunk $chunkNumber/${transfer.chunkEnvelopes.size}",
                )
                pendingTransfer = transfer
                updateState {
                    it.copy(
                        retryableInterruptionArmed = false,
                        armedFaultInjectionMode = null,
                        routeSyncState = RouteSyncState.FAILED,
                        transferProgress = transfer.toProgress(),
                        lastSyncResult = transfer.lastError ?: "Transfer interrupted",
                    )
                }
                return
            }

            if (transfer.usingLiveTransport && !bluetoothClient.isReady) {
                transfer = failTransfer(
                    transfer = transfer,
                    detail = "BLE transport disconnected before chunk $chunkNumber/${transfer.chunkEnvelopes.size} could be written",
                    restartFromFirstChunk = false,
                )
                return
            }

            val envelope = transfer.chunkEnvelopes[transfer.nextChunkIndex]
            if (transfer.usingLiveTransport) {
                val writeResult = runCatching { bluetoothClient.write(BleRouteSyncPacket.Chunk(envelope)) }
                if (writeResult.isFailure) {
                    transfer = failTransfer(
                        transfer = transfer,
                        detail = writeResult.exceptionOrNull()?.localizedMessage ?: "BLE write failed",
                        restartFromFirstChunk = false,
                    )
                    return
                }
            }

            transfer = transfer.copy(nextChunkIndex = chunkNumber, lastError = null)
            pendingTransfer = transfer
            updateState {
                it.copy(
                    transferProgress = transfer.toProgress(),
                    lastSyncResult = "Transferred chunk $chunkNumber/${transfer.chunkEnvelopes.size} (${transfer.toProgress().percentComplete}%)",
                )
            }
        }

        if (transfer.usingLiveTransport) {
            if (!bluetoothClient.isReady) {
                failTransfer(
                    transfer = transfer,
                    detail = "BLE transport disconnected after chunk upload finished; full transfer must be replayed",
                    restartFromFirstChunk = true,
                )
                return
            }
            updateState {
                it.copy(
                    routeSyncState = RouteSyncState.AWAITING_ACK,
                    lastSyncResult = "Waiting for ESP32 acknowledgement over BLE",
                )
            }
            scheduleAckTimeout()
        } else {
            simulateDeviceCompletion(transfer)
        }
    }

    private suspend fun simulateDeviceCompletion(transfer: PendingTransfer) {
        receiveStatus(
            RouteStatusMessage(
                routeIdentifier = transfer.routeIdentifier,
                revision = transfer.routeRevision,
                status = RouteSyncStatusCode.ACCEPTED,
                detail = "Checksum ${transfer.checksumHex} verified after ${transfer.chunkEnvelopes.size} chunks",
            ),
        )
        receiveStatus(
            RouteStatusMessage(
                routeIdentifier = transfer.routeIdentifier,
                revision = transfer.routeRevision,
                status = RouteSyncStatusCode.APPLYING,
                detail = "Applying route revision ${transfer.routeRevision ?: 0} on device",
            ),
        )
        receiveStatus(finalStatusFor(transfer))
    }

    private suspend fun handleInbound(message: RouteSyncMessage) {
        val decodedMessage = decodeInboundSyncMessage(message)
        updateState { it.copy(lastInboundMessage = decodedMessage) }
        when (decodedMessage) {
            is RouteSyncMessage.Status -> applyStatus(decodedMessage.message)
            is RouteSyncMessage.RerouteRequest -> {
                updateState {
                    it.copy(lastSyncResult = "Device requested reroute for ${decodedMessage.message.routeIdentifier}")
                }
            }
            is RouteSyncMessage.Set,
            is RouteSyncMessage.Update,
            is RouteSyncMessage.Clear,
            -> updateState { it.copy(lastSyncResult = "Received unexpected inbound ${decodedMessage.kindLabel} message") }
        }
    }

    private fun applyStatus(status: RouteStatusMessage) {
        when (status.status) {
            RouteSyncStatusCode.ACCEPTED,
            RouteSyncStatusCode.APPLYING,
            -> {
                updateState {
                    it.copy(
                        lastStatusCode = status.status,
                        routeSyncState = RouteSyncState.AWAITING_ACK,
                        lastSyncResult = status.detail ?: defaultStatusDetail(status.status),
                    )
                }
                scheduleAckTimeout()
            }
            RouteSyncStatusCode.ACTIVE -> {
                cancelAckTimeout()
                updateState {
                    it.copy(
                        lastStatusCode = status.status,
                        routeSyncState = RouteSyncState.SYNCED,
                        pendingRouteIdentifier = null,
                        pendingRouteRevision = null,
                        activeRouteIdentifier = status.routeIdentifier,
                        activeRouteRevision = status.revision,
                        activeRouteChecksumHex = pendingTransfer?.checksumHex ?: it.activeRouteChecksumHex,
                        transferProgress = null,
                        armedFaultInjectionMode = null,
                        lastSyncResult = status.detail ?: defaultStatusDetail(status.status),
                    )
                }
                pendingTransfer = null
            }
            RouteSyncStatusCode.CLEARED -> {
                cancelAckTimeout()
                updateState {
                    it.copy(
                        lastStatusCode = status.status,
                        routeSyncState = RouteSyncState.IDLE,
                        pendingRouteIdentifier = null,
                        pendingRouteRevision = null,
                        activeRouteIdentifier = null,
                        activeRouteRevision = null,
                        activeRouteChecksumHex = null,
                        transferProgress = null,
                        armedFaultInjectionMode = null,
                        lastSyncResult = status.detail ?: defaultStatusDetail(status.status),
                    )
                }
                pendingTransfer = null
            }
            RouteSyncStatusCode.RETRYABLE_FAILURE -> {
                cancelAckTimeout()
                pendingTransfer = pendingTransfer?.copy(
                    retryCount = (pendingTransfer?.retryCount ?: 0) + 1,
                    nextChunkIndex = 0,
                    lastError = status.detail ?: defaultStatusDetail(status.status),
                )
                updateState {
                    it.copy(
                        lastStatusCode = status.status,
                        routeSyncState = RouteSyncState.FAILED,
                        transferProgress = pendingTransfer?.toProgress() ?: it.transferProgress,
                        armedFaultInjectionMode = null,
                        lastSyncResult = status.detail ?: defaultStatusDetail(status.status),
                    )
                }
            }
            RouteSyncStatusCode.REJECTED,
            RouteSyncStatusCode.FATAL_FAILURE,
            -> {
                cancelAckTimeout()
                updateState {
                    it.copy(
                        lastStatusCode = status.status,
                        routeSyncState = RouteSyncState.FAILED,
                        pendingRouteIdentifier = null,
                        pendingRouteRevision = null,
                        transferProgress = null,
                        armedFaultInjectionMode = null,
                        lastSyncResult = status.detail ?: defaultStatusDetail(status.status),
                    )
                }
                pendingTransfer = null
            }
        }
    }

    private fun handleConnectionStateChange(state: DeviceConnectionState, name: String?) {
        updateState {
            it.copy(
                connectionState = state,
                lastDeviceName = name ?: it.lastDeviceName,
                routeSyncState = if (state == DeviceConnectionState.DISCONNECTED && pendingTransfer == null) {
                    RouteSyncState.IDLE
                } else {
                    it.routeSyncState
                },
            )
        }
        if (state == DeviceConnectionState.DISCONNECTED && pendingTransfer?.usingLiveTransport == true) {
            failTransfer(
                transfer = pendingTransfer ?: return,
                detail = "BLE transport disconnected; reconnect and resume the pending transfer",
                restartFromFirstChunk = (pendingTransfer?.nextChunkIndex ?: 0) >= (pendingTransfer?.chunkEnvelopes?.size ?: 0),
            )
        }
    }

    private fun finalStatusFor(transfer: PendingTransfer): RouteStatusMessage {
        return when (val message = transfer.message) {
            is RouteSyncMessage.Clear -> {
                RouteStatusMessage(
                    routeIdentifier = transfer.routeIdentifier,
                    revision = null,
                    status = RouteSyncStatusCode.CLEARED,
                    detail = "Device cleared active route",
                )
            }
            is RouteSyncMessage.Set -> finalRouteStatus(message.message.route, transfer.checksumHex, transfer.message.kindLabel, transfer.usingLiveTransport)
            is RouteSyncMessage.Update -> finalRouteStatus(message.message.route, transfer.checksumHex, transfer.message.kindLabel, transfer.usingLiveTransport)
            is RouteSyncMessage.Status,
            is RouteSyncMessage.RerouteRequest,
            -> {
                RouteStatusMessage(
                    routeIdentifier = transfer.routeIdentifier,
                    revision = transfer.routeRevision,
                    status = RouteSyncStatusCode.FATAL_FAILURE,
                    detail = "Unsupported outbound sync message kind ${transfer.message.kindLabel}",
                )
            }
        }
    }

    private fun finalRouteStatus(
        route: NormalizedRoutePackage,
        checksumHex: String,
        kindLabel: String,
        usingLiveTransport: Boolean,
    ): RouteStatusMessage {
        val activeRouteIdentifier = mutableState.value.activeRouteIdentifier
        val activeRouteRevision = mutableState.value.activeRouteRevision
        if (activeRouteIdentifier == route.routeIdentifier && activeRouteRevision != null) {
            if (route.revision < activeRouteRevision) {
                return RouteStatusMessage(
                    routeIdentifier = route.routeIdentifier,
                    revision = route.revision,
                    status = RouteSyncStatusCode.REJECTED,
                    detail = "Rejected stale route revision ${route.revision}; device already has rev $activeRouteRevision",
                )
            }
            if (route.revision == activeRouteRevision) {
                if (mutableState.value.activeRouteChecksumHex == checksumHex) {
                    return RouteStatusMessage(
                        routeIdentifier = route.routeIdentifier,
                        revision = route.revision,
                        status = RouteSyncStatusCode.ACTIVE,
                        detail = "Duplicate $kindLabel replay deduped; existing route kept active",
                    )
                }
                return RouteStatusMessage(
                    routeIdentifier = route.routeIdentifier,
                    revision = route.revision,
                    status = RouteSyncStatusCode.FATAL_FAILURE,
                    detail = "Revision conflict: route ${route.routeIdentifier} rev ${route.revision} has a different checksum",
                )
            }
        }
        return RouteStatusMessage(
            routeIdentifier = route.routeIdentifier,
            revision = route.revision,
            status = RouteSyncStatusCode.ACTIVE,
            detail = if (usingLiveTransport) {
                "Route revision ${route.revision} applied on ESP32 over Android BLE via $kindLabel"
            } else {
                "Route revision ${route.revision} applied over simulated BLE via $kindLabel"
            },
        )
    }

    private fun decodeInboundSyncMessage(message: RouteSyncMessage): RouteSyncMessage {
        return runCatching {
            val packet = BleRouteSyncPacket.SyncMessage(message)
            val decodedPacket = BleRouteSyncCodec.decode(BleRouteSyncCodec.encode(packet))
            (decodedPacket as? BleRouteSyncPacket.SyncMessage)?.message ?: message
        }.getOrElse { message }
    }

    private fun failTransfer(
        transfer: PendingTransfer,
        detail: String,
        restartFromFirstChunk: Boolean,
    ): PendingTransfer {
        cancelAckTimeout()
        val failedTransfer = transfer.copy(
            retryCount = transfer.retryCount + 1,
            lastError = detail,
            nextChunkIndex = if (restartFromFirstChunk || transfer.nextChunkIndex >= transfer.chunkEnvelopes.size) 0 else transfer.nextChunkIndex,
        )
        pendingTransfer = failedTransfer
        updateState {
            it.copy(
                routeSyncState = RouteSyncState.FAILED,
                transferProgress = failedTransfer.toProgress(),
                armedFaultInjectionMode = null,
                lastSyncResult = detail,
            )
        }
        return failedTransfer
    }

    private fun scheduleAckTimeout() {
        cancelAckTimeout()
        ackTimeoutJob = serviceScope.launch {
            delay(2_000)
            val transfer = pendingTransfer ?: return@launch
            if (mutableState.value.routeSyncState != RouteSyncState.AWAITING_ACK) return@launch
            val restartedTransfer = transfer.copy(
                retryCount = transfer.retryCount + 1,
                nextChunkIndex = 0,
                lastError = "Timed out waiting for ESP32 acknowledgement; replay the route transfer",
            )
            pendingTransfer = restartedTransfer
            updateState {
                it.copy(
                    routeSyncState = RouteSyncState.FAILED,
                    transferProgress = restartedTransfer.toProgress(),
                    armedFaultInjectionMode = null,
                    lastSyncResult = restartedTransfer.lastError ?: "Timed out waiting for device acknowledgement",
                )
            }
        }
    }

    private fun cancelAckTimeout() {
        ackTimeoutJob?.cancel()
        ackTimeoutJob = null
    }

    private fun defaultStatusDetail(status: RouteSyncStatusCode): String {
        return when (status) {
            RouteSyncStatusCode.ACCEPTED -> "Device accepted route payload"
            RouteSyncStatusCode.APPLYING -> "Device is applying route payload"
            RouteSyncStatusCode.ACTIVE -> "Device activated route"
            RouteSyncStatusCode.CLEARED -> "Device cleared route"
            RouteSyncStatusCode.REJECTED -> "Device rejected route payload"
            RouteSyncStatusCode.RETRYABLE_FAILURE -> "Device reported retryable sync failure"
            RouteSyncStatusCode.FATAL_FAILURE -> "Device reported sync failure"
        }
    }

    private fun updateState(transform: (SyncSessionState) -> SyncSessionState) {
        mutableState.value = transform(mutableState.value)
    }

    private data class PendingTransfer(
        val identifier: String,
        val message: RouteSyncMessage,
        val payload: ByteArray,
        val checksumHex: String,
        val chunkEnvelopes: List<RouteTransferChunkEnvelope>,
        val nextChunkIndex: Int,
        val retryCount: Int,
        val lastError: String?,
        val usingLiveTransport: Boolean,
    ) {
        val routeIdentifier: String?
            get() = when (message) {
                is RouteSyncMessage.Set -> message.message.route.routeIdentifier
                is RouteSyncMessage.Update -> message.message.routeIdentifier
                is RouteSyncMessage.Clear -> message.message.routeIdentifier
                is RouteSyncMessage.Status -> message.message.routeIdentifier
                is RouteSyncMessage.RerouteRequest -> message.message.routeIdentifier
            }

        val routeRevision: Int?
            get() = when (message) {
                is RouteSyncMessage.Set -> message.message.route.revision
                is RouteSyncMessage.Update -> message.message.revision
                is RouteSyncMessage.Clear,
                is RouteSyncMessage.RerouteRequest,
                -> null
                is RouteSyncMessage.Status -> message.message.revision
            }

        fun toProgress(): RouteTransferProgress {
            return RouteTransferProgress(
                transferIdentifier = identifier,
                messageKind = message.kindLabel,
                routeIdentifier = routeIdentifier,
                routeRevision = routeRevision,
                payloadBytes = payload.size,
                chunkSizeBytes = CHUNK_SIZE_BYTES,
                totalChunks = chunkEnvelopes.size,
                acknowledgedChunks = nextChunkIndex,
                retryCount = retryCount,
                checksumHex = checksumHex,
                resumeChunkIndex = nextChunkIndex.takeIf { it < chunkEnvelopes.size },
                lastError = lastError,
            )
        }
    }

    companion object {
        private const val CHUNK_SIZE_BYTES = 96
    }
}
