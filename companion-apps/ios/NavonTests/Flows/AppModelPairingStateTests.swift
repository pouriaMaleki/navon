import XCTest
@testable import Navon

@MainActor
final class AppModelPairingStateTests: XCTestCase {

    private func freshDefaults() -> UserDefaults {
        UserDefaults(suiteName: "appmodel-pairing-tests-\(UUID().uuidString)")!
    }

    private func sampleRecord() -> PairedPeripheralRecord {
        PairedPeripheralRecord(
            identifier: "B6C8AE6A-1A8C-4F2E-9F1A-9D2B0C3E4F5A",
            friendlyName: "Navon",
            pairedAt: CompanionPersistence.iso8601MillisFormatter.date(from: "2026-04-28T12:34:56.789Z")!
        )
    }

    func test_appModel_loadsPairedPeripheralOnInit_whenStored() {
        let defaults = freshDefaults()
        let persistence = CompanionPersistence(defaults: defaults)
        persistence.savePairedPeripheral(sampleRecord())

        let appModel = AppModel(persistence: persistence)
        XCTAssertEqual(appModel.deviceManager.pairedPeripheral, sampleRecord())
    }

    func test_appModel_pairedPeripheralIsNilWhenNothingStored() {
        let persistence = CompanionPersistence(defaults: freshDefaults())
        let appModel = AppModel(persistence: persistence)
        XCTAssertNil(appModel.deviceManager.pairedPeripheral)
    }

    func test_forgetPairedDevice_clearsBothMemoryAndPersistence() {
        let defaults = freshDefaults()
        let persistence = CompanionPersistence(defaults: defaults)
        persistence.savePairedPeripheral(sampleRecord())
        let appModel = AppModel(persistence: persistence)
        XCTAssertNotNil(appModel.deviceManager.pairedPeripheral)

        appModel.deviceManager.forgetPairedDevice()

        XCTAssertNil(appModel.deviceManager.pairedPeripheral)
        XCTAssertNil(CompanionPersistence(defaults: defaults).loadPairedPeripheral())
    }

    func test_beginPairingFlow_setsPairingStateInstructions() {
        let appModel = AppModel(persistence: CompanionPersistence(defaults: freshDefaults()))
        XCTAssertEqual(appModel.deviceManager.pairingState, .idle)
        appModel.deviceManager.beginPairingFlow()
        XCTAssertEqual(appModel.deviceManager.pairingState, .instructions)
    }
}
