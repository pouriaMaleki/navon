import XCTest
@testable import ESP32MapCompanion

final class PairingQrPayloadTests: XCTestCase {

    private let validJson = #"""
    {
      "v": 1,
      "id_ios": "B6C8AE6A-1A8C-4F2E-9F1A-9D2B0C3E4F5A",
      "id_android": "AA:BB:CC:DD:EE:FF",
      "secret": "AAECAwQFBgcICQoLDA0ODxAREhMUFRYXGBkaGxwdHh8=",
      "fw": "1.2.3"
    }
    """#

    private let expectedSecret = Data((UInt8(0)...UInt8(31)).map { $0 })

    func test_decode_validV1Payload() throws {
        let payload = try PairingQrPayload.decode(validJson)
        XCTAssertEqual(payload.peripheralIdentifier, "B6C8AE6A-1A8C-4F2E-9F1A-9D2B0C3E4F5A")
        XCTAssertEqual(payload.ephemeralSecret, expectedSecret)
        XCTAssertEqual(payload.firmwareVersion, "1.2.3")
    }

    func test_decode_rejectsMissingSecret() {
        let json = #"""
        { "v": 1, "id_ios": "B6C8AE6A-1A8C-4F2E-9F1A-9D2B0C3E4F5A" }
        """#
        XCTAssertThrowsError(try PairingQrPayload.decode(json)) { error in
            XCTAssertEqual(error as? PairingQrError, .missingField("secret"))
        }
    }

    func test_decode_rejectsMalformedSecret() {
        let json = #"""
        { "v": 1, "id_ios": "B6C8AE6A-1A8C-4F2E-9F1A-9D2B0C3E4F5A", "secret": "not-base64!@#" }
        """#
        XCTAssertThrowsError(try PairingQrPayload.decode(json)) { error in
            XCTAssertEqual(error as? PairingQrError, .malformedField("secret"))
        }
    }

    func test_decode_rejectsMalformedUuid() {
        let json = #"""
        { "v": 1, "id_ios": "not-a-uuid", "secret": "AAECAwQFBgcICQoLDA0ODxAREhMUFRYXGBkaGxwdHh8=" }
        """#
        XCTAssertThrowsError(try PairingQrPayload.decode(json)) { error in
            XCTAssertEqual(error as? PairingQrError, .malformedField("id_ios"))
        }
    }

    func test_decode_rejectsUnsupportedVersion() {
        let json = #"""
        { "v": 99, "id_ios": "B6C8AE6A-1A8C-4F2E-9F1A-9D2B0C3E4F5A", "secret": "AAECAwQFBgcICQoLDA0ODxAREhMUFRYXGBkaGxwdHh8=" }
        """#
        XCTAssertThrowsError(try PairingQrPayload.decode(json)) { error in
            XCTAssertEqual(error as? PairingQrError, .unsupportedVersion(99))
        }
    }

    func test_decode_acceptsMissingFirmwareVersion() throws {
        let json = #"""
        { "v": 1, "id_ios": "B6C8AE6A-1A8C-4F2E-9F1A-9D2B0C3E4F5A", "secret": "AAECAwQFBgcICQoLDA0ODxAREhMUFRYXGBkaGxwdHh8=" }
        """#
        let payload = try PairingQrPayload.decode(json)
        XCTAssertNil(payload.firmwareVersion)
        XCTAssertEqual(payload.ephemeralSecret, expectedSecret)
    }

    func test_decode_decodesParityFixture() throws {
        // Cross-platform contract: Android writes the same JSON shape. Decoding
        // straight from the fixture bytes guarantees the iOS struct hasn't
        // drifted on field names or base64 handling.
        let fixtureURL = URL(fileURLWithPath: #filePath)
            .deletingLastPathComponent()
            .deletingLastPathComponent()
            .deletingLastPathComponent()
            .deletingLastPathComponent()
            .appendingPathComponent("parity-fixtures/data/pairing_qr_v1.json")
        let data = try Data(contentsOf: fixtureURL)
        let json = String(data: data, encoding: .utf8)!
        let payload = try PairingQrPayload.decode(json)
        XCTAssertEqual(payload.peripheralIdentifier, "B6C8AE6A-1A8C-4F2E-9F1A-9D2B0C3E4F5A")
        XCTAssertEqual(payload.ephemeralSecret, expectedSecret)
        XCTAssertEqual(payload.firmwareVersion, "1.2.3")
    }
}
