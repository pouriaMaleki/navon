package me.fiksu.esp32map.companion.integration.ble

import android.content.Context
import java.util.UUID
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import me.fiksu.esp32map.companion.domain.DeviceConnectionState
import me.fiksu.esp32map.companion.domain.NormalizedRoutePackage
import me.fiksu.esp32map.companion.domain.RouteClearMessage
import me.fiksu.esp32map.companion.domain.RouteRerouteRequestMessage
import me.fiksu.esp32map.companion.domain.RouteSetMessage
import me.fiksu.esp32map.companion.domain.RouteStatusMessage
import me.fiksu.esp32map.companion.domain.RouteSyncMessage
import me.fiksu.esp32map.companion.domain.RouteSyncState
import me.fiksu.esp32map.companion.domain.RouteSyncStatusCode
import me.fiksu.esp32map.companion.domain.RouteSyncTransport
import me.fiksu.esp32map.companion.domain.RouteTransferProgress
import me.fiksu.esp32map.companion.domain.RouteUpdateMessage
import me.fiksu.esp32map.companion.domain.SyncSessionState

class BleRouteSyncService(
    context: Context,
    private val bluetoothClient: AndroidBleRouteSyncClient = AndroidBleRouteSyncClient(context),
) : RouteSyncTransport {
    private val mutableState = MutableStateFlow(SyncSessionState())
    val state: StateFlow<SyncSessionState> = mutableState.asStateFlow()

    private var pendingTransfer: PendingTransfer? = null

    init {
        bluetoothClient.onConnectionStateChange = { state, name ->
            updateState {
                it.copy(
                    connectionState = state,
                    lastDeviceName = name ?: it.lastDeviceName,
                    routeSyncState = if (state == DeviceConnectionState.DISCONNECTED) RouteSyncState.IDLE else it.routeSyncState,
                )
            }
        }
        bluetoothClient.onSyncMessage = { message ->
            kotlinx.coroutines.runBlocking {
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
        updateState {
            it.copy(
                routeSyncState = RouteSyncState.TRANSFERRING,
                lastSyncResult = "Resuming ${transfer.message.kindLabel} at chunk ${transfer.nextChunkIndex + 1}/${transfer.chunkEnvelopes.size}",
            )
        }
        drainPendingTransfer()
    }

    override fun armRetryableInterruptionOnNextTransfer() {
        updateState {
            it.copy(
                retryableInterruptionArmed = true,
                lastSyncResult = "Next transfer will simulate one retryable BLE interruption",
            )
        }
    }

    override suspend fun receiveStatus(message: RouteStatusMessage) {
        handleInbound(RouteSyncMessage.Status(message))
    }

    override suspend fun receiveRerouteRequest(message: RouteRerouteRequestMessage) {
        handleInbound(RouteSyncMessage.RerouteRequest(message))
    }

    private suspend fun beginTransfer(message: RouteSyncMessage) {
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
                        routeSyncState = RouteSyncState.FAILED,
                        transferProgress = transfer.toProgress(),
                        lastSyncResult = transfer.lastError ?: "Transfer interrupted",
                    )
                }
                return
            }

            val envelope = transfer.chunkEnvelopes[transfer.nextChunkIndex]
            if (bluetoothClient.isReady) {
                bluetoothClient.write(BleRouteSyncPacket.Chunk(envelope))
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

        if (bluetoothClient.isReady) {
            updateState {
                it.copy(
                    routeSyncState = RouteSyncState.AWAITING_ACK,
                    lastSyncResult = "Waiting for ESP32 acknowledgement over BLE",
                )
            }
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
        updateState {
            it.copy(
                lastStatusCode = status.status,
                routeSyncState = when (status.status) {
                    RouteSyncStatusCode.ACCEPTED,
                    RouteSyncStatusCode.APPLYING,
                    -> RouteSyncState.AWAITING_ACK
                    RouteSyncStatusCode.ACTIVE -> RouteSyncState.SYNCED
                    RouteSyncStatusCode.CLEARED -> RouteSyncState.IDLE
                    RouteSyncStatusCode.RETRYABLE_FAILURE,
                    RouteSyncStatusCode.REJECTED,
                    RouteSyncStatusCode.FATAL_FAILURE,
                    -> RouteSyncState.FAILED
                },
                pendingRouteIdentifier = when (status.status) {
                    RouteSyncStatusCode.ACTIVE,
                    RouteSyncStatusCode.CLEARED,
                    RouteSyncStatusCode.REJECTED,
                    RouteSyncStatusCode.FATAL_FAILURE,
                    -> null
                    else -> it.pendingRouteIdentifier
                },
                pendingRouteRevision = when (status.status) {
                    RouteSyncStatusCode.ACTIVE,
                    RouteSyncStatusCode.CLEARED,
                    RouteSyncStatusCode.REJECTED,
                    RouteSyncStatusCode.FATAL_FAILURE,
                    -> null
                    else -> it.pendingRouteRevision
                },
                activeRouteIdentifier = when (status.status) {
                    RouteSyncStatusCode.ACTIVE -> status.routeIdentifier
                    RouteSyncStatusCode.CLEARED -> null
                    else -> it.activeRouteIdentifier
                },
                activeRouteRevision = when (status.status) {
                    RouteSyncStatusCode.ACTIVE -> status.revision
                    RouteSyncStatusCode.CLEARED -> null
                    else -> it.activeRouteRevision
                },
                activeRouteChecksumHex = when (status.status) {
                    RouteSyncStatusCode.ACTIVE -> pendingTransfer?.checksumHex ?: it.activeRouteChecksumHex
                    RouteSyncStatusCode.CLEARED -> null
                    else -> it.activeRouteChecksumHex
                },
                transferProgress = when (status.status) {
                    RouteSyncStatusCode.ACTIVE,
                    RouteSyncStatusCode.CLEARED,
                    RouteSyncStatusCode.REJECTED,
                    RouteSyncStatusCode.FATAL_FAILURE,
                    -> null
                    else -> it.transferProgress
                },
                lastSyncResult = status.detail ?: defaultStatusDetail(status.status),
            )
        }
        if (status.status == RouteSyncStatusCode.ACTIVE || status.status == RouteSyncStatusCode.CLEARED || status.status == RouteSyncStatusCode.REJECTED || status.status == RouteSyncStatusCode.FATAL_FAILURE) {
            pendingTransfer = null
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
            is RouteSyncMessage.Set -> finalRouteStatus(message.message.route, transfer.checksumHex, transfer.message.kindLabel)
            is RouteSyncMessage.Update -> finalRouteStatus(message.message.route, transfer.checksumHex, transfer.message.kindLabel)
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
            detail = if (bluetoothClient.isReady) {
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
