package me.fiksu.esp32map.companion.integration.ble

import me.fiksu.esp32map.companion.domain.DeviceConnectionState
import me.fiksu.esp32map.companion.domain.RouteSyncFaultInjectionMode
import me.fiksu.esp32map.companion.domain.RouteSyncMessage

/**
 * Test double for [RouteSyncBluetoothClient]. Records what the service
 * called for it and lets the test inject results / errors. Mirrors the
 * shape of `companion-ios/.../FakeRouteSyncBluetoothClient.swift` so the
 * cross-platform pairing-flow tests stay in lockstep.
 */
class FakeRouteSyncBluetoothClient : RouteSyncBluetoothClient {
    override var onSyncMessage: ((RouteSyncMessage) -> Unit)? = null
    override var onConnectionStateChange: ((DeviceConnectionState, String?) -> Unit)? = null

    var isReadyOverride: Boolean = false

    override val isReady: Boolean
        get() = isReadyOverride

    var scanCallCount = 0
        private set
    var connectCallCount = 0
        private set
    var connectToPairedCallCount = 0
        private set
    var writeCallCount = 0
        private set
    var lastWrittenPacket: BleRouteSyncPacket? = null
        private set
    var lastConnectedIdentifier: String? = null
        private set

    var scanResult: Result<String> = Result.success("ESP32 Bike Minimap")
    var connectResult: Result<String> = Result.success("ESP32 Bike Minimap")
    var connectPairedResult: Result<String> = Result.success("ESP32 Bike Minimap")
    var connectAdvertisedResult: Result<String> = Result.success("ESP32 Bike Minimap")
    var writeResult: Result<Unit> = Result.success(Unit)
    var writePairingConfirmResult: Result<Unit> = Result.success(Unit)

    var connectToAdvertisedCallCount = 0
        private set
    var lastAdvertisedIdentifier: String? = null
        private set
    var writePairingConfirmCallCount = 0
        private set
    var lastWrittenPairingSecret: ByteArray? = null
        private set

    override fun armDebugFault(mode: RouteSyncFaultInjectionMode) {
        // No-op: fault injection isn't observable through this fake yet.
    }

    override suspend fun scanForRouteSyncPeripheral(timeoutMs: Long): String {
        scanCallCount += 1
        return scanResult.getOrThrow()
    }

    override suspend fun connectToScannedPeripheral(): String {
        connectCallCount += 1
        return connectResult.getOrThrow()
    }

    override suspend fun connectToPairedPeripheral(identifier: String): String {
        connectToPairedCallCount += 1
        lastConnectedIdentifier = identifier
        return connectPairedResult.getOrThrow()
    }

    override suspend fun connectToAdvertisedPeripheral(identifier: String): String {
        connectToAdvertisedCallCount += 1
        lastAdvertisedIdentifier = identifier
        return connectAdvertisedResult.getOrThrow()
    }

    override suspend fun writePairingConfirm(secret: ByteArray) {
        writePairingConfirmCallCount += 1
        lastWrittenPairingSecret = secret
        writePairingConfirmResult.getOrThrow()
    }

    override suspend fun write(packet: BleRouteSyncPacket) {
        writeCallCount += 1
        lastWrittenPacket = packet
        writeResult.getOrThrow()
    }
}
