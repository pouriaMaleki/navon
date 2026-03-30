package me.fiksu.esp32map.companion.integration.ble

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
import me.fiksu.esp32map.companion.domain.RouteUpdateMessage
import me.fiksu.esp32map.companion.domain.SyncSessionState

class BleRouteSyncService : RouteSyncTransport {
    private val mutableState = MutableStateFlow(SyncSessionState())
    val state: StateFlow<SyncSessionState> = mutableState.asStateFlow()

    override suspend fun scanForDevices() {
        mutableState.value = mutableState.value.copy(
            connectionState = DeviceConnectionState.SCANNING,
            lastDeviceName = "ESP32 Bike Minimap",
        )
    }

    override suspend fun connectToLastKnownDevice() {
        mutableState.value = mutableState.value.copy(connectionState = DeviceConnectionState.CONNECTING)
        mutableState.value = mutableState.value.copy(
            connectionState = DeviceConnectionState.CONNECTED,
            lastDeviceName = mutableState.value.lastDeviceName ?: "ESP32 Bike Minimap",
        )
    }

    override suspend fun publishSet(route: NormalizedRoutePackage) {
        mutableState.value = mutableState.value.copy(
            routeSyncState = RouteSyncState.PREPARING,
            lastOutboundMessage = RouteSyncMessage.Set(RouteSetMessage(route)),
        )
        mutableState.value = mutableState.value.copy(routeSyncState = RouteSyncState.TRANSFERRING)
        mutableState.value = mutableState.value.copy(routeSyncState = RouteSyncState.AWAITING_ACK)
        receiveStatus(
            RouteStatusMessage(
                routeIdentifier = route.routeIdentifier,
                revision = route.revision,
                status = RouteSyncStatusCode.ACTIVE,
                detail = "Route applied over BLE",
            ),
        )
    }

    override suspend fun publishUpdate(route: NormalizedRoutePackage) {
        mutableState.value = mutableState.value.copy(
            routeSyncState = RouteSyncState.PREPARING,
            lastOutboundMessage = RouteSyncMessage.Update(
                RouteUpdateMessage(
                    routeIdentifier = route.routeIdentifier,
                    revision = route.revision,
                    route = route,
                ),
            ),
        )
        mutableState.value = mutableState.value.copy(routeSyncState = RouteSyncState.TRANSFERRING)
        mutableState.value = mutableState.value.copy(routeSyncState = RouteSyncState.AWAITING_ACK)
        receiveStatus(
            RouteStatusMessage(
                routeIdentifier = route.routeIdentifier,
                revision = route.revision,
                status = RouteSyncStatusCode.ACTIVE,
                detail = "Replacement route applied over BLE",
            ),
        )
    }

    override suspend fun publishClear(routeIdentifier: String?) {
        mutableState.value = mutableState.value.copy(
            routeSyncState = RouteSyncState.PREPARING,
            lastOutboundMessage = RouteSyncMessage.Clear(RouteClearMessage(routeIdentifier)),
        )
        mutableState.value = mutableState.value.copy(routeSyncState = RouteSyncState.TRANSFERRING)
        receiveStatus(
            RouteStatusMessage(
                routeIdentifier = routeIdentifier,
                revision = null,
                status = RouteSyncStatusCode.CLEARED,
                detail = "Route cleared on device",
            ),
        )
    }

    override suspend fun receiveStatus(message: RouteStatusMessage) {
        mutableState.value = mutableState.value.copy(
            lastInboundMessage = RouteSyncMessage.Status(message),
            lastStatusCode = message.status,
        )
        when (message.status) {
            RouteSyncStatusCode.ACCEPTED,
            RouteSyncStatusCode.APPLYING,
            -> {
                mutableState.value = mutableState.value.copy(
                    routeSyncState = RouteSyncState.AWAITING_ACK,
                    lastSyncResult = message.detail ?: "Waiting for device acknowledgement",
                )
            }

            RouteSyncStatusCode.ACTIVE,
            -> {
                mutableState.value = mutableState.value.copy(
                    routeSyncState = RouteSyncState.SYNCED,
                    activeRouteIdentifier = message.routeIdentifier,
                    activeRouteRevision = message.revision,
                    lastSyncResult = message.detail ?: "Device activated route",
                )
            }

            RouteSyncStatusCode.CLEARED,
            -> {
                mutableState.value = mutableState.value.copy(
                    routeSyncState = RouteSyncState.IDLE,
                    activeRouteIdentifier = null,
                    activeRouteRevision = null,
                    lastSyncResult = message.detail ?: "Device cleared route",
                )
            }

            RouteSyncStatusCode.REJECTED,
            RouteSyncStatusCode.RETRYABLE_FAILURE,
            RouteSyncStatusCode.FATAL_FAILURE,
            -> {
                mutableState.value = mutableState.value.copy(
                    routeSyncState = RouteSyncState.FAILED,
                    lastSyncResult = message.detail ?: "Device reported sync failure",
                )
            }
        }
    }

    override suspend fun receiveRerouteRequest(message: RouteRerouteRequestMessage) {
        mutableState.value = mutableState.value.copy(
            lastInboundMessage = RouteSyncMessage.RerouteRequest(message),
            lastSyncResult = "Device requested reroute for ${message.routeIdentifier}",
        )
    }
}
