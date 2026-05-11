package app.navon.bike.integration.persistence

import android.app.Application
import androidx.test.core.app.ApplicationProvider
import com.google.gson.Gson
import app.navon.bike.domain.PairedPeripheralRecord
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner

/**
 * Persistence-layer tests for [PairedPeripheralRecord]. Uses
 * Robolectric so the tests run against a real `SharedPreferences`
 * implementation rather than the in-memory fallback path — that's the
 * one production hits, and the one Gson schema drift would surface in.
 */
@RunWith(RobolectricTestRunner::class)
class CompanionPersistencePairedPeripheralTest {

    private fun freshContext(): Application {
        // ApplicationProvider returns the same Application across tests
        // in the same JVM, but JUnit clears the SharedPreferences via the
        // class-isolation that Robolectric provides per test method.
        return ApplicationProvider.getApplicationContext()
    }

    private fun sampleRecord() = PairedPeripheralRecord(
        identifier = "AA:BB:CC:DD:EE:FF",
        friendlyName = "Navon",
        pairedAt = "2026-04-28T12:34:56.789Z",
    )

    @Test
    fun savePairedPeripheral_roundTrips() {
        val record = sampleRecord()
        val saver = CompanionPersistence(freshContext())
        saver.savePairedPeripheral(record)

        val loader = CompanionPersistence(freshContext())
        val loaded = loader.loadPairedPeripheral()

        assertEquals(
            "all three fields must round-trip exactly through Gson + SharedPreferences",
            record,
            loaded,
        )
    }

    @Test
    fun clearPairedPeripheral_removesRecord() {
        val saver = CompanionPersistence(freshContext())
        saver.savePairedPeripheral(sampleRecord())
        saver.clearPairedPeripheral()

        val reloaded = CompanionPersistence(freshContext()).loadPairedPeripheral()
        assertNull(
            "clear must remove the SharedPreferences key, not just blank the in-memory cache",
            reloaded,
        )
    }

    @Test
    fun loadPairedPeripheral_returnsNullWhenAbsent() {
        // Fresh context — never saved anything. Locks the unpaired
        // first-launch behaviour: the load path must not crash on a
        // missing key.
        assertNull(CompanionPersistence(freshContext()).loadPairedPeripheral())
    }

    @Test
    fun savePairedPeripheral_overwritesPriorRecord() {
        val a = PairedPeripheralRecord(
            identifier = "AA:AA:AA:AA:AA:AA",
            friendlyName = "First",
            pairedAt = "2026-04-01T00:00:00.000Z",
        )
        val b = PairedPeripheralRecord(
            identifier = "BB:BB:BB:BB:BB:BB",
            friendlyName = "Second",
            pairedAt = "2026-04-28T00:00:00.000Z",
        )
        val persistence = CompanionPersistence(freshContext())
        persistence.savePairedPeripheral(a)
        persistence.savePairedPeripheral(b)

        assertEquals(
            "single-bond at the persistence layer: a fresh save must replace any prior record",
            b,
            persistence.loadPairedPeripheral(),
        )
    }

    @Test
    fun loadPairedPeripheral_decodesParityFixture() {
        // Exact-bytes-of-the-parity-fixture decode. Catches Android-side
        // Gson behaviour drifting from the cross-platform spec at
        // `parity-fixtures/data/paired_peripheral.json`. Both
        // companion-android and companion-ios must accept the same
        // bytes; iOS has its own equivalent test in the iOS plan.
        val fixture = """{
            "identifier": "AA:BB:CC:DD:EE:FF",
            "friendlyName": "Navon",
            "pairedAt": "2026-04-28T12:34:56.789Z"
        }""".trimIndent()
        val decoded = Gson().fromJson(fixture, PairedPeripheralRecord::class.java)
        assertEquals(
            PairedPeripheralRecord(
                identifier = "AA:BB:CC:DD:EE:FF",
                friendlyName = "Navon",
                pairedAt = "2026-04-28T12:34:56.789Z",
            ),
            decoded,
        )
    }
}
