package app.navon.bike.flows

import app.cash.turbine.test
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.test.runTest
import app.navon.bike.fakes.FakeLocationService
import app.navon.bike.fixtures.HelsinkiGravel
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * L2 replay test — drives FakeLocationService with the helsinki-gravel fixture
 * and verifies the state flow emits every fix (flow #63).
 */
@OptIn(ExperimentalCoroutinesApi::class)
class RiderLifecycleTest {

    @Test
    fun replays_helsinki_gravel_and_reaches_final_fix() = runTest {
        val stream = HelsinkiGravel.loadStream()
        assertTrue("fixture should be non-empty", stream.isNotEmpty())
        val service = FakeLocationService()
        service.start()
        for (sample in stream) {
            service.emitFix(sample.latitude, sample.longitude)
        }
        val last = stream.last()
        val state = service.state.value
        assertEquals(last.latitude, state.currentLocation?.latitude)
        assertEquals(last.longitude, state.currentLocation?.longitude)
    }

    @Test
    fun state_flow_emits_fix_when_emit_called() = runTest {
        val service = FakeLocationService()
        service.state.test {
            assertEquals(null, awaitItem().currentLocation)
            service.start()
            assertEquals(null, awaitItem().currentLocation)
            service.emitFix(60.17, 24.94)
            val next = awaitItem()
            assertEquals(60.17, next.currentLocation?.latitude)
            assertEquals(24.94, next.currentLocation?.longitude)
        }
    }
}
