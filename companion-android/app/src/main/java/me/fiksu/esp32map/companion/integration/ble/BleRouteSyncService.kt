package me.fiksu.esp32map.companion.integration.ble

import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import me.fiksu.esp32map.companion.domain.DeviceConnectionState
import me.fiksu.esp32map.companion.domain.NormalizedRoutePackage
import me.fiksu.esp32map.companion.domain.RouteSyncState
import me.fiksu.esp32map.companion.domain.RouteSyncTransport
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

    override suspend fun sendRoute(route: NormalizedRoutePackage) {
        mutableState.value = mutableState.value.copy(routeSyncState = RouteSyncState.TRANSFERRING)
        mutableState.value = mutableState.value.copy(routeSyncState = RouteSyncState.AWAITING_ACK)
        mutableState.value = mutableState.value.copy(
            routeSyncState = RouteSyncState.SYNCED,
            lastSyncResult = "Synced route ${route.routeIdentifier} rev ${route.revision}",
        )
    }

    override suspend fun clearRoute(routeIdentifier: String?) {
        mutableState.value = mutableState.value.copy(
            routeSyncState = RouteSyncState.IDLE,
            lastSyncResult = "Cleared route ${routeIdentifier ?: "current"}",
        )
    }
}
