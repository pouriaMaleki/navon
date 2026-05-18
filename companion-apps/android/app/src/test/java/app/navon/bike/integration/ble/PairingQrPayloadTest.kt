package app.navon.bike.integration.ble

import androidx.test.core.app.ApplicationProvider
import androidx.test.ext.junit.runners.AndroidJUnit4
import java.io.File
import org.junit.Assert.assertArrayEquals
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.fail
import org.junit.Test
import org.junit.runner.RunWith

/**
 * Tests for the cross-platform pairing-QR JSON codec. The decoder must accept
 * the canonical fixture in `parity-fixtures/data/pairing_qr_v1.json` and reject
 * malformed / future-version payloads with specific error variants so the UI
 * can show actionable messages.
 */
@RunWith(AndroidJUnit4::class)
class PairingQrPayloadTest {

    @Test
    fun decode_validV1Payload() {
        val json = """{
            "v":1,
            "id_android":"AA:BB:CC:DD:EE:FF",
            "secret":"$SECRET_BASE64_OF_42S",
            "fw":"0.1.0"
        }""".trimIndent()
        val decoded = PairingQrPayload.decode(json)
        assertEquals("AA:BB:CC:DD:EE:FF", decoded.peripheralIdentifier)
        assertEquals(32, decoded.ephemeralSecret.size)
        assertArrayEquals(ByteArray(32) { 0x42 }, decoded.ephemeralSecret)
        assertEquals("0.1.0", decoded.firmwareVersion)
    }

    @Test
    fun decode_lowercaseMacIsCanonicalizedToUppercase() {
        // The QR's BD_ADDR may render as lowercase depending on the firmware
        // formatter; the decoder should canonicalize so downstream compare /
        // persistence uses one shape.
        val json = """{"v":1,"id_android":"aa:bb:cc:dd:ee:ff","secret":"$SECRET_BASE64_OF_42S","fw":"0.1.0"}"""
        val decoded = PairingQrPayload.decode(json)
        assertEquals("AA:BB:CC:DD:EE:FF", decoded.peripheralIdentifier)
    }

    @Test
    fun decode_rejectsMissingSecret() {
        val json = """{"v":1,"id_android":"AA:BB:CC:DD:EE:FF","fw":"0.1.0"}"""
        try {
            PairingQrPayload.decode(json)
            fail("expected MissingField(\"secret\")")
        } catch (e: PairingQrError.MissingField) {
            assertEquals("secret", e.field)
        }
    }

    @Test
    fun decode_rejectsMalformedSecret() {
        val json = """{"v":1,"id_android":"AA:BB:CC:DD:EE:FF","secret":"not-base64!@#","fw":"0.1.0"}"""
        try {
            PairingQrPayload.decode(json)
            fail("expected MalformedField(\"secret\")")
        } catch (e: PairingQrError.MalformedField) {
            assertEquals("secret", e.field)
        }
    }

    @Test
    fun decode_rejectsMalformedAndroidIdentifier() {
        val json = """{"v":1,"id_android":"not-a-mac","secret":"$SECRET_BASE64_OF_42S","fw":"0.1.0"}"""
        try {
            PairingQrPayload.decode(json)
            fail("expected MalformedField(\"id_android\")")
        } catch (e: PairingQrError.MalformedField) {
            assertEquals("id_android", e.field)
        }
    }

    @Test
    fun decode_rejectsUnsupportedVersion() {
        val json = """{"v":99,"id_android":"AA:BB:CC:DD:EE:FF","secret":"$SECRET_BASE64_OF_42S","fw":"0.1.0"}"""
        try {
            PairingQrPayload.decode(json)
            fail("expected UnsupportedVersion(99)")
        } catch (e: PairingQrError.UnsupportedVersion) {
            assertEquals(99, e.version)
        }
    }

    @Test
    fun decode_acceptsMissingFirmwareVersion() {
        val json = """{"v":1,"id_android":"AA:BB:CC:DD:EE:FF","secret":"$SECRET_BASE64_OF_42S"}"""
        val decoded = PairingQrPayload.decode(json)
        assertNull(decoded.firmwareVersion)
    }

    @Test
    fun decode_decodesParityFixture() {
        // Cross-platform contract: both Android and iOS decoders must agree on
        // the canonical fixture. Catches Gson behavior drifting from the
        // documented schema.
        // Tests run with cwd = `companion-android/app/`; the parity fixtures
        // live two levels up at the repo root.
        val fixture = File("../../../data/parity-fixtures/data/pairing_qr_v1.json").readText()
        val decoded = PairingQrPayload.decode(fixture)
        assertEquals("AA:BB:CC:DD:EE:FF", decoded.peripheralIdentifier)
        assertArrayEquals(ByteArray(32) { 0x42 }, decoded.ephemeralSecret)
        assertEquals("0.1.0", decoded.firmwareVersion)
    }

    @Test
    fun decode_rejectsInvalidJson() {
        try {
            PairingQrPayload.decode("not even json")
            fail("expected MalformedJson")
        } catch (e: PairingQrError.MalformedJson) {
            // ok
        }
    }

    private companion object {
        // base64(0x42 × 32) = 44 chars including the single `=` pad.
        private val SECRET_BASE64_OF_42S: String =
            "QkJCQkJCQkJCQkJCQkJCQkJCQkJCQkJCQkJCQkJCQkI="
    }
}
