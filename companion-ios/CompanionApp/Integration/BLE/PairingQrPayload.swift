import Foundation

/// Decoded form of the QR payload the firmware emits during pairing mode.
/// Cross-platform contract — see `parity-fixtures/data/pairing_qr_v1.json`
/// and `docs/ble-route-sync-contract.md`.
///
/// iOS does not get a usable peripheral identifier from the QR — `id_android`
/// is the BD_ADDR (CoreBluetooth surfaces a per-app UUID, not the MAC), and
/// the firmware does not emit `id_ios`. Instead, iOS scans for the route-sync
/// service UUID, connects to whichever peripheral advertises it while the
/// device is in pairing mode, and writes `ephemeralSecret` to the
/// pairing-confirm characteristic. The persisted `peripheral.identifier` is
/// captured AT CONNECT TIME, not from the QR.
struct PairingQrPayload: Equatable {
    let ephemeralSecret: Data
    let firmwareVersion: String?
    /// Optional. Surfaced for diagnostics only; iOS does not use this to
    /// match peripherals.
    let androidIdentifier: String?

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
        guard let secretBase64 = raw.secret else {
            throw PairingQrError.missingField("secret")
        }
        guard let secretData = Data(base64Encoded: secretBase64), secretData.count == 32 else {
            throw PairingQrError.malformedField("secret")
        }
        return PairingQrPayload(
            ephemeralSecret: secretData,
            firmwareVersion: raw.fw,
            androidIdentifier: raw.id_android
        )
    }

    private struct WireV1: Codable {
        // Snake_case mirrors the firmware's emitted JSON verbatim so Codable
        // picks up the wire-format keys directly.
        let v: Int
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
