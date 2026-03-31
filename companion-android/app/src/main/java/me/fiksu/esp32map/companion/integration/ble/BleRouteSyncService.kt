package me.fiksu.esp32map.companion.integration.ble

import java.util.UUID
import kotlin.math.ceil
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

class BleRouteSyncService : RouteSyncTransport {
    private val mutableState = MutableStateFlow(SyncSessionState())
    val state: StateFlow<SyncSessionState> = mutableState.asStateFlow()

    private var pendingTransfer: PendingTransfer? = null

    override suspend fun scanForDevices() {
        updateState {
            it.copy(
                connectionState = DeviceConnectionState.SCANNING,
                lastDeviceName = "ESP32 Bike Minimap",
            )
        }
    }

    override suspend fun connectToLastKnownDevice() {
        updateState { it.copy(connectionState = DeviceConnectionState.CONNECTING) }
        updateState {
            it.copy(
                connectionState = DeviceConnectionState.CONNECTED,
                lastDeviceName = it.lastDeviceName ?: "ESP32 Bike Minimap",
            )
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
                lastSyncResult = "Resuming ${transfer.message.kindLabel} at chunk ${transfer.nextChunkIndex + 1}/${transfer.totalChunks}",
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
        val decodedMessage = decodeInboundSyncMessage(RouteSyncMessage.Status(message))
        val statusMessage = decodedMessage as? RouteSyncMessage.Status ?: RouteSyncMessage.Status(message)
        updateState {
            it.copy(
                lastInboundMessage = decodedMessage,
                lastStatusCode = statusMessage.message.status,
            )
        }
        when (statusMessage.message.status) {
            RouteSyncStatusCode.ACCEPTED,
            RouteSyncStatusCode.APPLYING,
            -> {
                updateState {
                    it.copy(
                        routeSyncState = RouteSyncState.AWAITING_ACK,
                        lastSyncResult = statusMessage.message.detail ?: "Waiting for device acknowledgement",
                    )
                }
            }

            RouteSyncStatusCode.ACTIVE,
            -> {
                updateState {
                    it.copy(
                        routeSyncState = RouteSyncState.SYNCED,
                        pendingRouteIdentifier = null,
                        pendingRouteRevision = null,
                        activeRouteIdentifier = statusMessage.message.routeIdentifier,
                        activeRouteRevision = statusMessage.message.revision,
                        activeRouteChecksumHex = pendingTransfer?.checksumHex ?: it.activeRouteChecksumHex,
                        transferProgress = null,
                        lastSyncResult = statusMessage.message.detail ?: "Device activated route",
                    )
                }
                pendingTransfer = null
            }

            RouteSyncStatusCode.CLEARED,
            -> {
                updateState {
                    it.copy(
                        routeSyncState = RouteSyncState.IDLE,
                        pendingRouteIdentifier = null,
                        pendingRouteRevision = null,
                        activeRouteIdentifier = null,
                        activeRouteRevision = null,
                        activeRouteChecksumHex = null,
                        transferProgress = null,
                        lastSyncResult = statusMessage.message.detail ?: "Device cleared route",
                    )
                }
                pendingTransfer = null
            }

            RouteSyncStatusCode.RETRYABLE_FAILURE,
            -> {
                updateState {
                    it.copy(
                        routeSyncState = RouteSyncState.FAILED,
                        lastSyncResult = statusMessage.message.detail ?: "Device reported retryable sync failure",
                    )
                }
            }

            RouteSyncStatusCode.REJECTED,
            RouteSyncStatusCode.FATAL_FAILURE,
            -> {
                updateState {
                    it.copy(
                        routeSyncState = RouteSyncState.FAILED,
                        pendingRouteIdentifier = null,
                        pendingRouteRevision = null,
                        transferProgress = null,
                        lastSyncResult = statusMessage.message.detail ?: "Device reported sync failure",
                    )
                }
                pendingTransfer = null
            }
        }
    }

    override suspend fun receiveRerouteRequest(message: RouteRerouteRequestMessage) {
        val decodedMessage = decodeInboundSyncMessage(RouteSyncMessage.RerouteRequest(message))
        val rerouteMessage = decodedMessage as? RouteSyncMessage.RerouteRequest ?: RouteSyncMessage.RerouteRequest(message)
        updateState {
            it.copy(
                lastInboundMessage = decodedMessage,
                lastSyncResult = "Device requested reroute for ${rerouteMessage.message.routeIdentifier}",
            )
        }
    }

    private suspend fun beginTransfer(message: RouteSyncMessage) {
        val payload = BleRouteSyncCodec.canonicalPayloadBytes(message)
        val totalChunks = maxOf(1, ceil(payload.size.toDouble() / CHUNK_SIZE_BYTES.toDouble()).toInt())
        val transfer = PendingTransfer(
            identifier = UUID.randomUUID().toString(),
            message = message,
            payload = payload,
            checksumHex = BleRouteSyncCodec.checksumHex(payload),
            totalChunks = totalChunks,
            nextChunkIndex = 0,
            retryCount = 0,
            lastError = null,
        )
        pendingTransfer = transfer
        updateState {
            it.copy(
                routeSyncState = RouteSyncState.PREPARING,
                pendingRouteIdentifier = routeIdentifierFor(message),
                pendingRouteRevision = routeRevisionFor(message),
                lastOutboundMessage = message,
                transferProgress = transfer.toProgress(),
                lastSyncResult = "Prepared ${message.kindLabel} payload (${payload.size} B across $totalChunks chunks)",
            )
        }
        drainPendingTransfer()
    }

    private suspend fun drainPendingTransfer() {
        var transfer = pendingTransfer ?: error("No pending transfer")
        updateState { it.copy(routeSyncState = RouteSyncState.TRANSFERRING) }

        while (transfer.nextChunkIndex < transfer.totalChunks) {
            delay(80)
            val chunkNumber = transfer.nextChunkIndex + 1
            if (mutableState.value.retryableInterruptionArmed) {
                transfer = transfer.copy(
                    retryCount = transfer.retryCount + 1,
                    lastError = "Simulated BLE interruption at chunk $chunkNumber/${transfer.totalChunks}",
                )
                pendingTransfer = transfer
                updateState {
                    it.copy(
                        retryableInterruptionArmed = false,
                        transferProgress = transfer.toProgress(),
                    )
                }
                receiveStatus(
                    RouteStatusMessage(
                        routeIdentifier = transfer.routeIdentifier,
                        revision = transfer.routeRevision,
                        status = RouteSyncStatusCode.RETRYABLE_FAILURE,
                        detail = "Transfer interrupted at chunk $chunkNumber/${transfer.totalChunks}; tap Resume pending transfer",
                    ),
                )
                return
            }

            transfer = transfer.copy(nextChunkIndex = chunkNumber, lastError = null)
            pendingTransfer = transfer
            updateState {
                it.copy(
                    transferProgress = transfer.toProgress(),
                    lastSyncResult = "Transferred chunk $chunkNumber/${transfer.totalChunks} (${transfer.toProgress().percentComplete}%)",
                )
            }
        }

        receiveStatus(
            RouteStatusMessage(
                routeIdentifier = transfer.routeIdentifier,
                revision = transfer.routeRevision,
                status = RouteSyncStatusCode.ACCEPTED,
                detail = "Checksum ${transfer.checksumHex} verified after ${transfer.totalChunks} chunks",
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
            detail = "Route revision ${route.revision} applied over BLE via $kindLabel",
        )
    }

    private fun routeIdentifierFor(message: RouteSyncMessage): String? {
        return when (message) {
            is RouteSyncMessage.Set -> message.message.route.routeIdentifier
            is RouteSyncMessage.Update -> message.message.routeIdentifier
            is RouteSyncMessage.Clear -> message.message.routeIdentifier
            is RouteSyncMessage.Status -> message.message.routeIdentifier
            is RouteSyncMessage.RerouteRequest -> message.message.routeIdentifier
        }
    }

    private fun routeRevisionFor(message: RouteSyncMessage): Int? {
        return when (message) {
            is RouteSyncMessage.Set -> message.message.route.revision
            is RouteSyncMessage.Update -> message.message.revision
            is RouteSyncMessage.Clear -> null
            is RouteSyncMessage.Status -> message.message.revision
            is RouteSyncMessage.RerouteRequest -> message.message.revision
        }
    }

    private fun canonicalPayloadString(message: RouteSyncMessage): String {
        return when (message) {
            is RouteSyncMessage.Set -> routePayloadString("set", message.message.route)
            is RouteSyncMessage.Update -> routePayloadString("update", message.message.route)
            is RouteSyncMessage.Clear -> listOf(
                "kind=clear",
                "route_id=${message.message.routeIdentifier ?: "current"}",
            ).joinToString("\n")
            is RouteSyncMessage.Status -> listOf(
                "kind=status",
                "route_id=${message.message.routeIdentifier ?: "none"}",
                "revision=${message.message.revision ?: "none"}",
                "status=${message.message.status.name.lowercase()}",
                "detail=${message.message.detail.orEmpty()}",
            ).joinToString("\n")
            is RouteSyncMessage.RerouteRequest -> listOf(
                "kind=reroute_request",
                "route_id=${message.message.routeIdentifier}",
                "revision=${message.message.revision}",
                "rider=${"%.6f,%.6f".format(message.message.riderLocation.latitude, message.message.riderLocation.longitude)}",
                "reason=${message.message.reason}",
            ).joinToString("\n")
        }
    }

    private fun routePayloadString(kind: String, route: NormalizedRoutePackage): String {
        val geometry = route.geometry.joinToString(";") { point ->
            "%.6f,%.6f".format(point.latitude, point.longitude)
        }
        val maneuvers = route.maneuvers.joinToString(";") { maneuver ->
            listOf(
                maneuver.id,
                maneuver.maneuverType.name.lowercase(),
                "%.1f".format(maneuver.distanceFromStartMeters),
                "%.6f,%.6f".format(maneuver.location.latitude, maneuver.location.longitude),
                maneuver.instructionText.orEmpty(),
            ).joinToString("|")
        }
        return listOf(
            "kind=$kind",
            "route_id=${route.routeIdentifier}",
            "revision=${route.revision}",
            "version=${route.version.major}.${route.version.minor}",
            "summary=${"%.1f|%d|%s|%s".format(route.summary.totalDistanceMeters, route.summary.estimatedDurationSeconds, route.summary.startLabel.orEmpty(), route.summary.destinationLabel.orEmpty())}",
            "geometry=$geometry",
            "maneuvers=$maneuvers",
            "provenance=${route.provenance.providerId.name.lowercase()}|${route.provenance.sourceReference.orEmpty()}|${route.provenance.generatedAtUnixMs}",
        ).joinToString("\n")
    }

    private fun checksumHex(payload: ByteArray): String {
        var hash = 2_166_136_261L
        payload.forEach { byte ->
            hash = hash xor (byte.toInt() and 0xff).toLong()
            hash = (hash * 16_777_619L) and 0xffff_ffffL
        }
        return "%08x".format(hash)
    }

    private fun decodeInboundSyncMessage(message: RouteSyncMessage): RouteSyncMessage {
        return runCatching {
            val packet = BleRouteSyncPacket.SyncMessage(message)
            val decodedPacket = BleRouteSyncCodec.decode(BleRouteSyncCodec.encode(packet))
            (decodedPacket as? BleRouteSyncPacket.SyncMessage)?.message ?: message
        }.getOrElse { message }
    }

    private fun updateState(transform: (SyncSessionState) -> SyncSessionState) {
        mutableState.value = transform(mutableState.value)
    }

    private data class PendingTransfer(
        val identifier: String,
        val message: RouteSyncMessage,
        val payload: ByteArray,
        val checksumHex: String,
        val totalChunks: Int,
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
                is RouteSyncMessage.Clear -> null
                is RouteSyncMessage.Status -> message.message.revision
                is RouteSyncMessage.RerouteRequest -> message.message.revision
            }

        fun toProgress(): RouteTransferProgress {
            return RouteTransferProgress(
                transferIdentifier = identifier,
                messageKind = message.kindLabel,
                routeIdentifier = routeIdentifier,
                routeRevision = routeRevision,
                payloadBytes = payload.size,
                chunkSizeBytes = CHUNK_SIZE_BYTES,
                totalChunks = totalChunks,
                acknowledgedChunks = nextChunkIndex,
                retryCount = retryCount,
                checksumHex = checksumHex,
                resumeChunkIndex = if (nextChunkIndex < totalChunks) nextChunkIndex else null,
                lastError = lastError,
            )
        }
    }

    companion object {
        private const val CHUNK_SIZE_BYTES = 96
    }
}
