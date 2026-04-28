import Foundation
import CoreBluetooth

/// Identification of the peripheral the client is currently connected to.
/// Returned from `connectToAdvertisedPeripheral` so the caller can persist
/// the CoreBluetooth `identifier` for the fast-path reconnect later.
struct ConnectedPeripheralInfo: Equatable {
    let name: String
    /// `CBPeripheral.identifier.uuidString`, stable across reconnects of the
    /// same paired peer on iOS. iOS does not get the BD_ADDR.
    let identifier: String
}

/// Surface of `CoreBluetoothRouteSyncClient` that `BleRouteSyncService` depends on.
/// Exists so unit tests can inject `FakeRouteSyncBluetoothClient` without spinning
/// up `CBCentralManager`. Callbacks may fire on a CoreBluetooth queue.
protocol RouteSyncBluetoothClient: AnyObject {
    var onSyncMessage: ((RouteSyncMessage) -> Void)? { get set }
    var onConnectionStateChange: ((DeviceConnectionState, String?) -> Void)? { get set }
    var isReady: Bool { get }

    func armDebugFault(_ mode: RouteSyncFaultInjectionMode)
    func scanForRouteSyncPeripheral(timeout: TimeInterval) async throws -> String
    func connectToScannedPeripheral() async throws -> String
    func connectToPairedPeripheral(identifier: String) async throws -> String
    /// Pairing-flow connect: scan by service UUID and connect to the first
    /// peripheral that advertises it (no identifier from QR — iOS only sees
    /// CoreBluetooth's per-app UUID once a peripheral is discovered).
    func connectToAdvertisedPeripheral() async throws -> ConnectedPeripheralInfo
    func writePairingConfirm(secret: Data) async throws
    func write(packet: BleRouteSyncPacket) async throws
}

enum CoreBluetoothRouteSyncError: LocalizedError {
    case bluetoothUnavailable
    case bluetoothUnauthorized
    case bluetoothPoweredOff
    case scanTimedOut
    case noDiscoveredPeripheral
    case serviceMissing
    case characteristicMissing
    case disconnected(String)
    case writeFailed(String)
    case invalidInboundPacket

    var errorDescription: String? {
        switch self {
        case .bluetoothUnavailable:
            return "CoreBluetooth is not available on this device"
        case .bluetoothUnauthorized:
            return "Bluetooth permission has not been granted"
        case .bluetoothPoweredOff:
            return "Bluetooth is powered off"
        case .scanTimedOut:
            return "No ESP32 route-sync peripheral was found before the scan timed out"
        case .noDiscoveredPeripheral:
            return "No scanned ESP32 route-sync peripheral is available to connect"
        case .serviceMissing:
            return "The ESP32 route-sync GATT service was not found"
        case .characteristicMissing:
            return "The ESP32 route-sync characteristics were not discovered"
        case .disconnected(let detail):
            return detail
        case .writeFailed(let detail):
            return detail
        case .invalidInboundPacket:
            return "Received an invalid BLE route-sync packet"
        }
    }
}

final class CoreBluetoothRouteSyncClient: NSObject, RouteSyncBluetoothClient {
    var onSyncMessage: ((RouteSyncMessage) -> Void)?
    var onConnectionStateChange: ((DeviceConnectionState, String?) -> Void)?

    private var armedDebugFault: RouteSyncFaultInjectionMode?

    private lazy var centralManager = CBCentralManager(delegate: self, queue: nil)
    private var discoveredPeripheral: CBPeripheral?
    private var connectedPeripheral: CBPeripheral?
    private var chunkWriteCharacteristic: CBCharacteristic?
    private var eventNotifyCharacteristic: CBCharacteristic?
    private var pairingConfirmCharacteristic: CBCharacteristic?

    private var pendingPowerOnContinuation: CheckedContinuation<Void, Error>?
    private var pendingScanContinuation: CheckedContinuation<String, Error>?
    private var pendingConnectContinuation: CheckedContinuation<String, Error>?
    private var pendingWriteContinuation: CheckedContinuation<Void, Error>?

    private let serviceUUID = CBUUID(string: BleRouteSyncGattContract.serviceUUID)
    private let chunkWriteUUID = CBUUID(string: BleRouteSyncGattContract.chunkWriteCharacteristicUUID)
    private let eventNotifyUUID = CBUUID(string: BleRouteSyncGattContract.eventNotifyCharacteristicUUID)
    private let pairingConfirmUUID = CBUUID(string: BleRouteSyncGattContract.pairingConfirmCharacteristicUUID)

    var isReady: Bool {
        connectedPeripheral != nil && chunkWriteCharacteristic != nil && eventNotifyCharacteristic != nil
    }

    func armDebugFault(_ mode: RouteSyncFaultInjectionMode) {
        armedDebugFault = mode
    }

    func scanForRouteSyncPeripheral(timeout: TimeInterval = 6.0) async throws -> String {
        try await ensurePoweredOn()
        if let connectedPeripheral {
            return connectedPeripheral.name ?? discoveredPeripheral?.name ?? "ESP32 Bike Minimap"
        }

        onConnectionStateChange?(.scanning, nil)
        return try await withCheckedThrowingContinuation { continuation in
            pendingScanContinuation = continuation
            centralManager.scanForPeripherals(withServices: [serviceUUID], options: [CBCentralManagerScanOptionAllowDuplicatesKey: false])
            Task { @MainActor in
                try? await Task.sleep(nanoseconds: UInt64(timeout * 1_000_000_000))
                if let pendingScanContinuation {
                    self.pendingScanContinuation = nil
                    self.centralManager.stopScan()
                    pendingScanContinuation.resume(throwing: CoreBluetoothRouteSyncError.scanTimedOut)
                    self.onConnectionStateChange?(.disconnected, nil)
                }
            }
        }
    }

    func connectToScannedPeripheral() async throws -> String {
        try await ensurePoweredOn()
        if isReady, let connectedPeripheral {
            return connectedPeripheral.name ?? "ESP32 Bike Minimap"
        }
        guard let peripheral = discoveredPeripheral ?? connectedPeripheral else {
            throw CoreBluetoothRouteSyncError.noDiscoveredPeripheral
        }
        return try await beginConnect(to: peripheral)
    }

    /// Reconnect to an already-paired peripheral by its `peripheral.identifier`
    /// without scanning. Falls through to `noDiscoveredPeripheral` so callers
    /// (`AppModel.connectToDevice`) can fall back to a full scan when the iOS
    /// bond store hasn't cached the peer yet (fresh install case).
    func connectToPairedPeripheral(identifier: String) async throws -> String {
        try await ensurePoweredOn()
        guard let uuid = UUID(uuidString: identifier) else {
            throw CoreBluetoothRouteSyncError.noDiscoveredPeripheral
        }
        let peripherals = centralManager.retrievePeripherals(withIdentifiers: [uuid])
        guard let peripheral = peripherals.first else {
            throw CoreBluetoothRouteSyncError.noDiscoveredPeripheral
        }
        return try await beginConnect(to: peripheral)
    }

    /// Pairing-flow connect: scan for the route-sync service UUID and connect
    /// to the first responder. iOS does not get the peripheral identifier
    /// from the QR — `id_android` is the BD_ADDR (CoreBluetooth surfaces a
    /// per-app UUID instead) and the firmware doesn't emit `id_ios`. The
    /// caller persists the returned `ConnectedPeripheralInfo.identifier` for
    /// future fast-path reconnects.
    func connectToAdvertisedPeripheral() async throws -> ConnectedPeripheralInfo {
        // Scan via the existing scan path so the timeout and continuation
        // semantics stay aligned with `scanForDevices()`.
        _ = try await scanForRouteSyncPeripheral(timeout: 6.0)
        let name = try await connectToScannedPeripheral()
        guard let identifier = connectedPeripheral?.identifier.uuidString else {
            throw CoreBluetoothRouteSyncError.noDiscoveredPeripheral
        }
        return ConnectedPeripheralInfo(name: name, identifier: identifier)
    }

    /// Send the QR-OOB confirmation secret to the device's pairing-confirm
    /// characteristic. Single-shot write that the firmware uses to gate the
    /// transition from pairing mode to operational mode.
    func writePairingConfirm(secret: Data) async throws {
        guard let peripheral = connectedPeripheral, let characteristic = pairingConfirmCharacteristic else {
            throw CoreBluetoothRouteSyncError.characteristicMissing
        }
        try await withCheckedThrowingContinuation { (continuation: CheckedContinuation<Void, Error>) in
            pendingWriteContinuation = continuation
            peripheral.writeValue(secret, for: characteristic, type: .withResponse)
        }
    }

    /// Shared post-discovery connect chain so the scan path and fast path
    /// can't drift on service/characteristic/notify wiring. Sets the connect
    /// continuation, fires connecting state, and lets CoreBluetooth's didConnect
    /// callback drive the rest of the handshake via `completeConnectIfReady`.
    private func beginConnect(to peripheral: CBPeripheral) async throws -> String {
        onConnectionStateChange?(.connecting, peripheral.name)
        peripheral.delegate = self
        return try await withCheckedThrowingContinuation { continuation in
            pendingConnectContinuation = continuation
            centralManager.connect(peripheral, options: nil)
        }
    }

    func write(packet: BleRouteSyncPacket) async throws {
        guard let peripheral = connectedPeripheral,
              let characteristic = chunkWriteCharacteristic else {
            throw CoreBluetoothRouteSyncError.characteristicMissing
        }

        if armedDebugFault == .writeFailure {
            armedDebugFault = nil
            throw CoreBluetoothRouteSyncError.writeFailed("Injected BLE write failure before packet send")
        }

        let disconnectAfterWrite = armedDebugFault == .disconnectAfterChunkWrite
        if disconnectAfterWrite {
            armedDebugFault = nil
        }

        let payload = BleRouteSyncCodec.encode(packet)
        let writeType: CBCharacteristicWriteType = characteristic.properties.contains(.write) ? .withResponse : .withoutResponse
        if writeType == .withResponse {
            try await withCheckedThrowingContinuation { continuation in
                pendingWriteContinuation = continuation
                peripheral.writeValue(payload, for: characteristic, type: .withResponse)
            }
        } else {
            peripheral.writeValue(payload, for: characteristic, type: .withoutResponse)
            try await Task.sleep(nanoseconds: 20_000_000)
        }

        if disconnectAfterWrite {
            centralManager.cancelPeripheralConnection(peripheral)
            throw CoreBluetoothRouteSyncError.disconnected("Injected BLE disconnect after chunk write")
        }
    }

    private func ensurePoweredOn() async throws {
        switch centralManager.state {
        case .poweredOn:
            return
        case .unauthorized:
            throw CoreBluetoothRouteSyncError.bluetoothUnauthorized
        case .poweredOff:
            throw CoreBluetoothRouteSyncError.bluetoothPoweredOff
        case .unsupported:
            throw CoreBluetoothRouteSyncError.bluetoothUnavailable
        case .resetting, .unknown:
            try await withCheckedThrowingContinuation { continuation in
                pendingPowerOnContinuation = continuation
            }
        @unknown default:
            throw CoreBluetoothRouteSyncError.bluetoothUnavailable
        }
    }

    private func completeConnectIfReady(for peripheral: CBPeripheral) {
        guard let services = peripheral.services else { return }
        guard let routeSyncService = services.first(where: { $0.uuid == serviceUUID }) else {
            if let pendingConnectContinuation {
                self.pendingConnectContinuation = nil
                pendingConnectContinuation.resume(throwing: CoreBluetoothRouteSyncError.serviceMissing)
            }
            return
        }
        guard let characteristics = routeSyncService.characteristics else { return }
        chunkWriteCharacteristic = characteristics.first(where: { $0.uuid == chunkWriteUUID })
        eventNotifyCharacteristic = characteristics.first(where: { $0.uuid == eventNotifyUUID })
        pairingConfirmCharacteristic = characteristics.first(where: { $0.uuid == pairingConfirmUUID })
        guard let eventNotifyCharacteristic else {
            if let pendingConnectContinuation {
                self.pendingConnectContinuation = nil
                pendingConnectContinuation.resume(throwing: CoreBluetoothRouteSyncError.characteristicMissing)
            }
            return
        }
        if !eventNotifyCharacteristic.isNotifying {
            peripheral.setNotifyValue(true, for: eventNotifyCharacteristic)
            return
        }
        let name = peripheral.name ?? "ESP32 Bike Minimap"
        if let pendingConnectContinuation {
            self.pendingConnectContinuation = nil
            pendingConnectContinuation.resume(returning: name)
        }
        onConnectionStateChange?(.connected, name)
    }
}

extension CoreBluetoothRouteSyncClient: CBCentralManagerDelegate {
    func centralManagerDidUpdateState(_ central: CBCentralManager) {
        switch central.state {
        case .poweredOn:
            if let pendingPowerOnContinuation {
                self.pendingPowerOnContinuation = nil
                pendingPowerOnContinuation.resume()
            }
        case .unauthorized:
            pendingPowerOnContinuation?.resume(throwing: CoreBluetoothRouteSyncError.bluetoothUnauthorized)
            pendingPowerOnContinuation = nil
        case .poweredOff:
            pendingPowerOnContinuation?.resume(throwing: CoreBluetoothRouteSyncError.bluetoothPoweredOff)
            pendingPowerOnContinuation = nil
            onConnectionStateChange?(.disconnected, connectedPeripheral?.name)
        case .unsupported:
            pendingPowerOnContinuation?.resume(throwing: CoreBluetoothRouteSyncError.bluetoothUnavailable)
            pendingPowerOnContinuation = nil
        default:
            break
        }
    }

    func centralManager(_ central: CBCentralManager, didDiscover peripheral: CBPeripheral, advertisementData: [String : Any], rssi RSSI: NSNumber) {
        discoveredPeripheral = peripheral
        central.stopScan()
        if let pendingScanContinuation {
            self.pendingScanContinuation = nil
            pendingScanContinuation.resume(returning: peripheral.name ?? "ESP32 Bike Minimap")
        }
    }

    func centralManager(_ central: CBCentralManager, didConnect peripheral: CBPeripheral) {
        connectedPeripheral = peripheral
        discoveredPeripheral = peripheral
        peripheral.delegate = self
        peripheral.discoverServices([serviceUUID])
    }

    func centralManager(_ central: CBCentralManager, didFailToConnect peripheral: CBPeripheral, error: Error?) {
        if let pendingConnectContinuation {
            self.pendingConnectContinuation = nil
            pendingConnectContinuation.resume(throwing: error ?? CoreBluetoothRouteSyncError.disconnected("Failed to connect to \(peripheral.name ?? "ESP32 device")"))
        }
        onConnectionStateChange?(.disconnected, peripheral.name)
    }

    func centralManager(_ central: CBCentralManager, didDisconnectPeripheral peripheral: CBPeripheral, error: Error?) {
        connectedPeripheral = nil
        chunkWriteCharacteristic = nil
        eventNotifyCharacteristic = nil
        let detail = error?.localizedDescription ?? "Disconnected from \(peripheral.name ?? "ESP32 device")"
        if let pendingConnectContinuation {
            self.pendingConnectContinuation = nil
            pendingConnectContinuation.resume(throwing: CoreBluetoothRouteSyncError.disconnected(detail))
        }
        if let pendingWriteContinuation {
            self.pendingWriteContinuation = nil
            pendingWriteContinuation.resume(throwing: CoreBluetoothRouteSyncError.disconnected(detail))
        }
        onConnectionStateChange?(.disconnected, peripheral.name)
    }
}

extension CoreBluetoothRouteSyncClient: CBPeripheralDelegate {
    func peripheral(_ peripheral: CBPeripheral, didDiscoverServices error: Error?) {
        if let error {
            pendingConnectContinuation?.resume(throwing: error)
            pendingConnectContinuation = nil
            return
        }
        guard let routeSyncService = peripheral.services?.first(where: { $0.uuid == serviceUUID }) else {
            pendingConnectContinuation?.resume(throwing: CoreBluetoothRouteSyncError.serviceMissing)
            pendingConnectContinuation = nil
            return
        }
        peripheral.discoverCharacteristics([chunkWriteUUID, eventNotifyUUID, pairingConfirmUUID], for: routeSyncService)
    }

    func peripheral(_ peripheral: CBPeripheral, didDiscoverCharacteristicsFor service: CBService, error: Error?) {
        if let error {
            pendingConnectContinuation?.resume(throwing: error)
            pendingConnectContinuation = nil
            return
        }
        completeConnectIfReady(for: peripheral)
    }

    func peripheral(_ peripheral: CBPeripheral, didUpdateNotificationStateFor characteristic: CBCharacteristic, error: Error?) {
        if let error {
            pendingConnectContinuation?.resume(throwing: error)
            pendingConnectContinuation = nil
            return
        }
        completeConnectIfReady(for: peripheral)
    }

    func peripheral(_ peripheral: CBPeripheral, didWriteValueFor characteristic: CBCharacteristic, error: Error?) {
        guard let pendingWriteContinuation else { return }
        self.pendingWriteContinuation = nil
        if let error {
            pendingWriteContinuation.resume(throwing: CoreBluetoothRouteSyncError.writeFailed(error.localizedDescription))
        } else {
            pendingWriteContinuation.resume()
        }
    }

    func peripheral(_ peripheral: CBPeripheral, didUpdateValueFor characteristic: CBCharacteristic, error: Error?) {
        guard characteristic.uuid == eventNotifyUUID else { return }
        guard error == nil, let value = characteristic.value else { return }
        do {
            let packet = try BleRouteSyncCodec.decode(value)
            guard case .syncMessage(let message) = packet else {
                throw CoreBluetoothRouteSyncError.invalidInboundPacket
            }
            if armedDebugFault == .dropNextInboundStatus, case .status = message {
                armedDebugFault = nil
                return
            }
            onSyncMessage?(message)
        } catch {
            onConnectionStateChange?(.connected, peripheral.name)
        }
    }
}
