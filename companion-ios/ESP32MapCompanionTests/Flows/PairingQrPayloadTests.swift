import XCTest
@testable import ESP32MapCompanion

final class PairingQrPayloadTests: XCTestCase {

    /// 32 bytes of `0x42` ("B"). Matches the canonical parity fixture
    /// `parity-fixtures/data/pairing_qr_v1.json` produced by the firmware
    /// during pairing-mode tests.
    private let expectedSecret = Data(repeating: 0x42, count: 32)

    private let validJson = #"""
    {"v":1,"id_android":"AA:BB:CC:DD:EE:FF","secret":"QkJCQkJCQkJCQkJCQkJCQkJCQkJCQkJCQkJCQkJCQkI=","fw":"0.1.0"}
    """#

    func test_decode_validV1Payload() throws {
        let payload = try PairingQrPayload.decode(validJson)
        XCTAssertEqual(payload.ephemeralSecret, expectedSecret)
        XCTAssertEqual(payload.firmwareVersion, "0.1.0")
        XCTAssertEqual(payload.androidIdentifier, "AA:BB:CC:DD:EE:FF")
    }

    func test_decode_rejectsMissingSecret() {
        let json = #"{ "v": 1, "id_android": "AA:BB:CC:DD:EE:FF" }"#
        XCTAssertThrowsError(try PairingQrPayload.decode(json)) { error in
            XCTAssertEqual(error as? PairingQrError, .missingField("secret"))
        }
    }

    func test_decode_rejectsMalformedSecret() {
        let json = #"{ "v": 1, "secret": "not-base64!@#" }"#
        XCTAssertThrowsError(try PairingQrPayload.decode(json)) { error in
            XCTAssertEqual(error as? PairingQrError, .malformedField("secret"))
        }
    }

    func test_decode_rejectsWrongLengthSecret() {
        // 16 bytes (base64 of 16 zero bytes) — must reject; firmware always
        // emits 32 bytes.
        let json = #"{ "v": 1, "secret": "AAAAAAAAAAAAAAAAAAAAAA==" }"#
        XCTAssertThrowsError(try PairingQrPayload.decode(json)) { error in
            XCTAssertEqual(error as? PairingQrError, .malformedField("secret"))
        }
    }

    func test_decode_rejectsUnsupportedVersion() {
        let json = #"{ "v": 99, "secret": "QkJCQkJCQkJCQkJCQkJCQkJCQkJCQkJCQkJCQkJCQkI=" }"#
        XCTAssertThrowsError(try PairingQrPayload.decode(json)) { error in
            XCTAssertEqual(error as? PairingQrError, .unsupportedVersion(99))
        }
    }

    func test_decode_acceptsMissingFirmwareVersion() throws {
        let json = #"{ "v": 1, "secret": "QkJCQkJCQkJCQkJCQkJCQkJCQkJCQkJCQkJCQkJCQkI=" }"#
        let payload = try PairingQrPayload.decode(json)
        XCTAssertNil(payload.firmwareVersion)
        XCTAssertEqual(payload.ephemeralSecret, expectedSecret)
    }

    func test_decode_acceptsMissingAndroidIdentifier() throws {
        // The iOS path doesn't need `id_android` at all; absence must not
        // fail decode (the firmware emits it but iOS would still accept a
        // future schema change that drops it).
        let json = #"{ "v": 1, "secret": "QkJCQkJCQkJCQkJCQkJCQkJCQkJCQkJCQkJCQkJCQkI=" }"#
        let payload = try PairingQrPayload.decode(json)
        XCTAssertNil(payload.androidIdentifier)
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
        XCTAssertEqual(payload.ephemeralSecret, expectedSecret)
        XCTAssertEqual(payload.firmwareVersion, "0.1.0")
        XCTAssertEqual(payload.androidIdentifier, "AA:BB:CC:DD:EE:FF")
    }
}
