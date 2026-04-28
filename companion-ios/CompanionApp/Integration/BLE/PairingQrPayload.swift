import Foundation

/// Decoded form of the QR payload the firmware emits during pairing mode.
/// Cross-platform contract — see `parity-fixtures/data/pairing_qr_v1.json`.
struct PairingQrPayload: Equatable {
    let peripheralIdentifier: String
    let ephemeralSecret: Data
    let firmwareVersion: String?

    static func decode(_ string: String) throws -> PairingQrPayload {
        guard let data = string.data(using: .utf8) else {
            throw PairingQrError.malformedField("payload")
        }
        let raw: WireV1
        do {
            raw = try JSONDecoder().decode(WireV1.self, from: data)
        } catch {
            throw PairingQrError.malformedField("payload")
        }

        guard raw.v == 1 else {
            throw PairingQrError.unsupportedVersion(raw.v)
        }
        guard let idIos = raw.id_ios else {
            throw PairingQrError.missingField("id_ios")
        }
        guard UUID(uuidString: idIos) != nil else {
            throw PairingQrError.malformedField("id_ios")
        }
        guard let secretBase64 = raw.secret else {
            throw PairingQrError.missingField("secret")
        }
        guard let secretData = Data(base64Encoded: secretBase64) else {
            throw PairingQrError.malformedField("secret")
        }
        return PairingQrPayload(
            peripheralIdentifier: idIos,
            ephemeralSecret: secretData,
            firmwareVersion: raw.fw
        )
    }

    private struct WireV1: Codable {
        // Note: Android-side IDs use snake_case; the decoder mirrors that
        // verbatim so Codable picks up the wire-format keys directly.
        let v: Int
        let id_ios: String?
        let id_android: String?
        let secret: String?
        let fw: String?
    }
}

enum PairingQrError: LocalizedError, Equatable {
    case unsupportedVersion(Int)
    case missingField(String)
    case malformedField(String)

    var errorDescription: String? {
        switch self {
        case .unsupportedVersion(let v):
            return "Pairing QR version \(v) is not supported by this app."
        case .missingField(let field):
            return "Pairing QR is missing required field: \(field)."
        case .malformedField(let field):
            return "Pairing QR has a malformed value for field: \(field)."
        }
    }
}
