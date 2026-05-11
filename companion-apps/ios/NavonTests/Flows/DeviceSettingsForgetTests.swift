import XCTest
@testable import Navon

@MainActor
final class DeviceSettingsForgetTests: XCTestCase {

    private func freshDefaults() -> UserDefaults {
        UserDefaults(suiteName: "device-settings-forget-tests-\(UUID().uuidString)")!
    }

    private func sampleRecord() -> PairedPeripheralRecord {
        PairedPeripheralRecord(
            identifier: "B6C8AE6A-1A8C-4F2E-9F1A-9D2B0C3E4F5A",
            friendlyName: "Navon",
            pairedAt: CompanionPersistence.iso8601MillisFormatter.date(from: "2026-04-28T12:34:56.789Z")!
        )
    }

    func test_forgetButton_clearsPersistedPairing() {
        let defaults = freshDefaults()
        let persistence = CompanionPersistence(defaults: defaults)
        persistence.savePairedPeripheral(sampleRecord())
        let appModel = AppModel(persistence: persistence)
        XCTAssertNotNil(appModel.pairedPeripheral)

        appModel.forgetPairedDevice()

        XCTAssertNil(appModel.pairedPeripheral)
        XCTAssertNil(CompanionPersistence(defaults: defaults).loadPairedPeripheral())
    }

    func test_pairedDeviceSection_descriptorWhenUnpaired() {
        let descriptor = DeviceSettingsSectionDescriptor.from(record: nil, connectionState: .disconnected)
        XCTAssertEqual(descriptor, .callToAction("Pair a new device"))
    }

    func test_pairedDeviceSection_descriptorWhenPaired() {
        let record = sampleRecord()
        let descriptor = DeviceSettingsSectionDescriptor.from(record: record, connectionState: .disconnected)
        XCTAssertEqual(
            descriptor,
            .detail(name: record.friendlyName, lastPairedAt: record.pairedAt, primaryAction: .connect)
        )
    }

    func test_pairedDeviceSection_descriptorWhenPairedAndConnected() {
        let record = sampleRecord()
        let descriptor = DeviceSettingsSectionDescriptor.from(record: record, connectionState: .connected)
        XCTAssertEqual(
            descriptor,
            .detail(name: record.friendlyName, lastPairedAt: record.pairedAt, primaryAction: .disconnect)
        )
    }
}
