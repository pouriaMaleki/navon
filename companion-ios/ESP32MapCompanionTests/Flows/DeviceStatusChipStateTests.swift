import XCTest
@testable import ESP32MapCompanion

@MainActor
final class DeviceStatusChipStateTests: XCTestCase {

    private func freshDefaults() -> UserDefaults {
        UserDefaults(suiteName: "device-chip-tests-\(UUID().uuidString)")!
    }

    private func sampleRecord() -> PairedPeripheralRecord {
        PairedPeripheralRecord(
            identifier: "B6C8AE6A-1A8C-4F2E-9F1A-9D2B0C3E4F5A",
            friendlyName: "ESP32 Bike Minimap",
            pairedAt: CompanionPersistence.iso8601MillisFormatter.date(from: "2026-04-28T12:34:56.789Z")!
        )
    }

    private func makeAppModel(
        seedRecord: PairedPeripheralRecord?,
        connectionState: DeviceConnectionState
    ) -> (AppModel, FakeRouteSyncBluetoothClient) {
        let persistence = CompanionPersistence(defaults: freshDefaults())
        if let seedRecord {
            persistence.savePairedPeripheral(seedRecord)
        }
        let fake = FakeRouteSyncBluetoothClient()
        let service = BleRouteSyncService(bluetoothClient: fake)
        let appModel = AppModel(persistence: persistence, bleService: service)
        // Force-set a session state by routing a fake connection-state event
        // through the same Combine pipeline AppModel observes.
        service.sessionState.connectionState = connectionState
        if let seedRecord {
            service.sessionState.lastDeviceName = seedRecord.friendlyName
        }
        return (appModel, fake)
    }

    private func makeViewModel(
        seedRecord: PairedPeripheralRecord? = nil,
        connectionState: DeviceConnectionState = .disconnected
    ) -> (HomeViewModel, AppModel, FakeRouteSyncBluetoothClient) {
        let (appModel, fake) = makeAppModel(seedRecord: seedRecord, connectionState: connectionState)
        let vm = HomeViewModel(appModel: appModel, placeSearchService: FakePlaceSearch())
        return (vm, appModel, fake)
    }

    func test_chipHidden_whenNoRecord() {
        // Single-bond model: chip is only rendered when a record exists. The
        // unpaired entry point lives in DeviceSettingsView. Catches the chip
        // accidentally surfacing a "tap to pair" affordance from home.
        for connectionState in [DeviceConnectionState.disconnected, .scanning, .connecting, .connected] {
            let (vm, _, _) = makeViewModel(seedRecord: nil, connectionState: connectionState)
            XCTAssertNil(vm.deviceChipState, "expected nil chip for connection \(connectionState)")
        }
    }

    func test_chipTap_whenUnpaired_isNoOp() async {
        // The chip never actually surfaces from home in the unpaired state,
        // but the VM tap handler must still be safe if called externally
        // (avoids a regression where an unrelated caller wakes the pair flow).
        let (vm, appModel, fake) = makeViewModel(seedRecord: nil, connectionState: .disconnected)
        XCTAssertEqual(appModel.pairingState, .idle)
        vm.handleDeviceChipTap()
        await Task.yield()
        XCTAssertEqual(appModel.pairingState, .idle)
        XCTAssertEqual(fake.scanCallCount + fake.connectToPairedCallCount, 0)
    }

    func test_pairedDisconnected_whenRecordButStateDisconnected() {
        let record = sampleRecord()
        let (vm, _, _) = makeViewModel(seedRecord: record, connectionState: .disconnected)
        XCTAssertEqual(vm.deviceChipState, .pairedDisconnected(name: record.friendlyName))
    }

    func test_connecting_whenScanning() {
        let record = sampleRecord()
        let (vm, _, _) = makeViewModel(seedRecord: record, connectionState: .scanning)
        XCTAssertEqual(vm.deviceChipState, .connecting(name: record.friendlyName))
    }

    func test_connecting_whenConnecting() {
        let record = sampleRecord()
        let (vm, _, _) = makeViewModel(seedRecord: record, connectionState: .connecting)
        XCTAssertEqual(vm.deviceChipState, .connecting(name: record.friendlyName))
    }

    func test_connected_whenConnectedAndRecordPresent() {
        let record = sampleRecord()
        let (vm, _, _) = makeViewModel(seedRecord: record, connectionState: .connected)
        XCTAssertEqual(vm.deviceChipState, .connected(name: record.friendlyName))
    }

    func test_chipTapAction_dispatchesCorrectAction() async {
        let record = sampleRecord()

        // Paired+disconnected → connectToDevice() (fast-path attempt then fallback).
        let (vmDisco, _, fakeD) = makeViewModel(seedRecord: record, connectionState: .disconnected)
        vmDisco.handleDeviceChipTap()
        await Task.yield()
        try? await Task.sleep(nanoseconds: 50_000_000)
        XCTAssertGreaterThan(fakeD.connectToPairedCallCount + fakeD.scanCallCount, 0)

        // Connected → toggles showConnectionPopover.
        let (vmConn, _, _) = makeViewModel(seedRecord: record, connectionState: .connected)
        XCTAssertFalse(vmConn.showConnectionPopover)
        vmConn.handleDeviceChipTap()
        XCTAssertTrue(vmConn.showConnectionPopover)

        // Connecting → no-op (popover stays false, pairingState stays idle).
        let (vmGoing, appModelG, fakeG) = makeViewModel(seedRecord: record, connectionState: .scanning)
        XCTAssertFalse(vmGoing.showConnectionPopover)
        vmGoing.handleDeviceChipTap()
        XCTAssertFalse(vmGoing.showConnectionPopover)
        XCTAssertEqual(appModelG.pairingState, .idle)
        XCTAssertEqual(fakeG.scanCallCount + fakeG.connectToPairedCallCount, 0)
    }
}
