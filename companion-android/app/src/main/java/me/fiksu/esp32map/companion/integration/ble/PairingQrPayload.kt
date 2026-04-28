package me.fiksu.esp32map.companion.integration.ble

import com.google.gson.JsonParser
import com.google.gson.JsonSyntaxException
import java.util.Base64

/**
 * Cross-platform pairing-QR wire format. Decoded from the JSON the firmware
 * shows on its panel during the QR-OOB pairing handshake. The schema lives in
 * `parity-fixtures/data/pairing_qr_v1.json`; both Android and iOS companions
 * decode the same JSON to keep the firmware emitting one canonical wire form.
 *
 * Schema (v1):
 * ```json
 * {"v":1,"id_android":"AA:BB:CC:DD:EE:FF","secret":"<base64-32B>","fw":"<semver>"}
 * ```
 *
 * - `id_android` — the BLE peripheral's BD_ADDR formatted as a colon MAC.
 *   iOS ignores this and matches by service-scan + secret instead.
 * - `secret` — base64-encoded 32-byte ephemeral secret. Written back to the
 *   firmware's `pairing_confirm` characteristic to close the handshake.
 * - `fw` — optional. Surfaced in diagnostics so the operator can spot a
 *   firmware/companion version mismatch at scan time.
 */
data class PairingQrPayload(
    val peripheralIdentifier: String,
    val ephemeralSecret: ByteArray,
    val firmwareVersion: String?,
) {
    init {
        require(ephemeralSecret.size == SECRET_LEN_BYTES) {
            "ephemeralSecret must be $SECRET_LEN_BYTES bytes, got ${ephemeralSecret.size}"
        }
    }

    override fun equals(other: Any?): Boolean {
        if (this === other) return true
        if (other !is PairingQrPayload) return false
        if (peripheralIdentifier != other.peripheralIdentifier) return false
        if (!ephemeralSecret.contentEquals(other.ephemeralSecret)) return false
        if (firmwareVersion != other.firmwareVersion) return false
        return true
    }

    override fun hashCode(): Int {
        var result = peripheralIdentifier.hashCode()
        result = 31 * result + ephemeralSecret.contentHashCode()
        result = 31 * result + (firmwareVersion?.hashCode() ?: 0)
        return result
    }

    companion object {
        const val SCHEMA_VERSION: Int = 1
        const val SECRET_LEN_BYTES: Int = 32
        private val MAC_PATTERN = Regex("^[0-9A-Fa-f]{2}(:[0-9A-Fa-f]{2}){5}$")

        fun decode(rawJson: String): PairingQrPayload {
            val root = try {
                JsonParser.parseString(rawJson)
            } catch (e: JsonSyntaxException) {
                throw PairingQrError.MalformedJson(e.message ?: "invalid JSON")
            }
            if (!root.isJsonObject) {
                throw PairingQrError.MalformedJson("payload root must be an object")
            }
            val obj = root.asJsonObject

            val versionElement = obj.get("v")
                ?: throw PairingQrError.MissingField("v")
            val version = try {
                versionElement.asInt
            } catch (e: Exception) {
                throw PairingQrError.MalformedField("v")
            }
            if (version != SCHEMA_VERSION) {
                throw PairingQrError.UnsupportedVersion(version)
            }

            val identifierElement = obj.get("id_android")
                ?: throw PairingQrError.MissingField("id_android")
            val identifier = try {
                identifierElement.asString
            } catch (e: Exception) {
                throw PairingQrError.MalformedField("id_android")
            }
            if (!MAC_PATTERN.matches(identifier)) {
                throw PairingQrError.MalformedField("id_android")
            }
            val canonicalIdentifier = identifier.uppercase()

            val secretElement = obj.get("secret")
                ?: throw PairingQrError.MissingField("secret")
            val secretBase64 = try {
                secretElement.asString
            } catch (e: Exception) {
                throw PairingQrError.MalformedField("secret")
            }
            val secret = try {
                Base64.getDecoder().decode(secretBase64.trim())
            } catch (e: IllegalArgumentException) {
                throw PairingQrError.MalformedField("secret")
            }
            if (secret.size != SECRET_LEN_BYTES) {
                throw PairingQrError.MalformedField("secret")
            }

            val firmwareVersion = obj.get("fw")?.let { element ->
                if (element.isJsonNull) null else element.asString
            }

            return PairingQrPayload(
                peripheralIdentifier = canonicalIdentifier,
                ephemeralSecret = secret,
                firmwareVersion = firmwareVersion,
            )
        }
    }
}

sealed class PairingQrError(message: String) : Exception(message) {
    data class MissingField(val field: String) :
        PairingQrError("pairing QR payload missing required field: $field")

    data class MalformedField(val field: String) :
        PairingQrError("pairing QR payload field is malformed: $field")

    data class UnsupportedVersion(val version: Int) :
        PairingQrError("pairing QR schema version $version is not supported by this companion")

    data class MalformedJson(val reason: String) :
        PairingQrError("pairing QR payload is not valid JSON: $reason")
}
