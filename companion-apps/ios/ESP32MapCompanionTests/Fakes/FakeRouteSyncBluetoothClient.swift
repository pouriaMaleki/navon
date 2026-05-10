import Foundation
@testable import ESP32MapCompanion

/// Test double for `RouteSyncBluetoothClient`. Records call counts and arguments
/// so service- and AppModel-level tests can assert on the BLE traffic without
/// touching CoreBluetooth.
final class FakeRouteSyncBluetoothClient: RouteSyncBluetoothClient {
    struct PhoneGpsWrite: Equatable {
        let lat: Double
        let lon: Double
        let speed: Double
        let course: Double?
        let accuracy: Double?
    }

    var onSyncMessage: ((RouteSyncMessage) -> Void)?
    var onConnectionStateChange: ((DeviceConnectionState, String?) -> Void)?

    var isReady: Bool = false

    private(set) var scanCallCount = 0
    private(set) var connectCallCount = 0
    private(set) var connectToPairedCallCount = 0
    private(set) var connectToAdvertisedCallCount = 0
    private(set) var writeCallCount = 0
    private(set) var writePairingConfirmCallCount = 0
    private(set) var writePairingRequestCallCount = 0
    private(set) var armDebugFaultCallCount = 0

    private(set) var lastScanTimeout: TimeInterval?
    private(set) var lastConnectedIdentifier: String?
    private(set) var lastWrittenPacket: BleRouteSyncPacket?
    private(set) var lastWrittenPairingSecret: Data?
    private(set) var lastArmedDebugFault: RouteSyncFaultInjectionMode?
    private(set) var phoneGpsWrites: [PhoneGpsWrite] = []

    /// What `scanForRouteSyncPeripheral` should produce: a device name on
    /// success or an error to throw. Defaults to a stub name.
    var scanResult: Result<String, Error> = .success("ESP32 Bike Minimap")
    var connectResult: Result<String, Error> = .success("ESP32 Bike Minimap")
    var connectToPairedResult: Result<String, Error> = .success("ESP32 Bike Minimap")
    var connectToAdvertisedResult: Result<ConnectedPeripheralInfo, Error> = .success(
        ConnectedPeripheralInfo(name: "ESP32 Bike Minimap", identifier: "B6C8AE6A-1A8C-4F2E-9F1A-9D2B0C3E4F5A")
    )
    var writeResult: Result<Void, Error> = .success(())
    var writePairingConfirmResult: Result<Void, Error> = .success(())
    var writePairingRequestResult: Result<Void, Error> = .success(())

    func armDebugFault(_ mode: RouteSyncFaultInjectionMode) {
        armDebugFaultCallCount += 1
        lastArmedDebugFault = mode
    }

    func scanForRouteSyncPeripheral(timeout: TimeInterval) async throws -> String {
        scanCallCount += 1
        lastScanTimeout = timeout
        return try scanResult.get()
    }

    func connectToScannedPeripheral() async throws -> String {
        connectCallCount += 1
        return try connectResult.get()
    }

    func connectToPairedPeripheral(identifier: String) async throws -> String {
        connectToPairedCallCount += 1
        lastConnectedIdentifier = identifier
        return try connectToPairedResult.get()
    }

    func connectToAdvertisedPeripheral() async throws -> ConnectedPeripheralInfo {
        connectToAdvertisedCallCount += 1
        let info = try connectToAdvertisedResult.get()
        lastConnectedIdentifier = info.identifier
        return info
    }

    func writePairingConfirm(secret: Data) async throws {
        writePairingConfirmCallCount += 1
        lastWrittenPairingSecret = secret
        try writePairingConfirmResult.get()
    }

    func writePairingRequest() async throws {
        writePairingRequestCallCount += 1
        try writePairingRequestResult.get()
    }

    func write(packet: BleRouteSyncPacket) async throws {
        writeCallCount += 1
        lastWrittenPacket = packet
        try writeResult.get()
    }

    func writePhoneGpsSample(lat: Double, lon: Double, speed: Double, course: Double?, accuracy: Double?) async throws {
        phoneGpsWrites.append(
            PhoneGpsWrite(
                lat: lat,
                lon: lon,
                speed: speed,
                course: course,
                accuracy: accuracy
            )
        )
    }
}
