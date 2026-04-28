import XCTest
@testable import ESP32MapCompanion

final class CompanionPersistencePairedPeripheralTests: XCTestCase {

    private func freshDefaults() -> UserDefaults {
        UserDefaults(suiteName: "paired-peripheral-tests-\(UUID().uuidString)")!
    }

    private func parityFixturePairedAt() -> Date {
        // 2026-04-28T12:34:56.789Z — matches paired_peripheral.json so the
        // round-trip through the same formatter we use in persistence stays
        // bit-for-bit equal across reloads.
        CompanionPersistence.iso8601MillisFormatter.date(from: "2026-04-28T12:34:56.789Z")!
    }

    func test_savePairedPeripheral_roundTrips() {
        let defaults = freshDefaults()
        let pairedAt = parityFixturePairedAt()
        let record = PairedPeripheralRecord(
            identifier: "B6C8AE6A-1A8C-4F2E-9F1A-9D2B0C3E4F5A",
            friendlyName: "ESP32 Bike Minimap",
            pairedAt: pairedAt
        )
        let writer = CompanionPersistence(defaults: defaults)
        writer.savePairedPeripheral(record)

        // Fresh persistence pointed at the same suite-scoped defaults to catch
        // any reliance on in-memory state.
        let reader = CompanionPersistence(defaults: defaults)
        let loaded = reader.loadPairedPeripheral()
        XCTAssertEqual(loaded?.identifier, record.identifier)
        XCTAssertEqual(loaded?.friendlyName, record.friendlyName)
        XCTAssertEqual(loaded?.pairedAt, record.pairedAt)
    }

    func test_clearPairedPeripheral_removesRecord() {
        let defaults = freshDefaults()
        let persistence = CompanionPersistence(defaults: defaults)
        persistence.savePairedPeripheral(
            PairedPeripheralRecord(
                identifier: "B6C8AE6A-1A8C-4F2E-9F1A-9D2B0C3E4F5A",
                friendlyName: "ESP32 Bike Minimap",
                pairedAt: parityFixturePairedAt()
            )
        )
        persistence.clearPairedPeripheral()
        XCTAssertNil(persistence.loadPairedPeripheral())
    }

    func test_loadPairedPeripheral_returnsNilWhenAbsent() {
        let persistence = CompanionPersistence(defaults: freshDefaults())
        XCTAssertNil(persistence.loadPairedPeripheral())
    }

    func test_savePairedPeripheral_overwritesPriorRecord() {
        let defaults = freshDefaults()
        let persistence = CompanionPersistence(defaults: defaults)
        let recordA = PairedPeripheralRecord(
            identifier: "AAAAAAAA-1A8C-4F2E-9F1A-9D2B0C3E4F5A",
            friendlyName: "Old",
            pairedAt: parityFixturePairedAt()
        )
        let recordB = PairedPeripheralRecord(
            identifier: "BBBBBBBB-1A8C-4F2E-9F1A-9D2B0C3E4F5A",
            friendlyName: "New",
            pairedAt: parityFixturePairedAt().addingTimeInterval(60)
        )
        persistence.savePairedPeripheral(recordA)
        persistence.savePairedPeripheral(recordB)
        XCTAssertEqual(persistence.loadPairedPeripheral(), recordB)
    }

    func test_loadPairedPeripheral_decodesParityFixture() throws {
        // Cross-platform contract: Android writes the same JSON shape. Decoding
        // straight from the fixture bytes guarantees the iOS Codable struct
        // hasn't drifted on field names or date encoding.
        let fixtureURL = URL(fileURLWithPath: #filePath)
            .deletingLastPathComponent()
            .deletingLastPathComponent()
            .deletingLastPathComponent()
            .deletingLastPathComponent()
            .appendingPathComponent("parity-fixtures/data/paired_peripheral.json")
        let data = try Data(contentsOf: fixtureURL)
        let decoder = JSONDecoder()
        decoder.dateDecodingStrategy = .formatted(CompanionPersistence.iso8601MillisFormatter)
        let record = try decoder.decode(PairedPeripheralRecord.self, from: data)
        XCTAssertEqual(record.identifier, "AA:BB:CC:DD:EE:FF")
        XCTAssertEqual(record.friendlyName, "ESP32 Bike Minimap")
        XCTAssertEqual(record.pairedAt, parityFixturePairedAt())
    }
}
