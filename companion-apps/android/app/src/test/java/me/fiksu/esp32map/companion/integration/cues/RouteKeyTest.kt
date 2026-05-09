package me.fiksu.esp32map.companion.integration.cues

import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertNotEquals
import org.junit.Test

// Bug 3: when a reroute returns the same routeIdentifier but a bumped
// revision, the CueSnapshot.routeId must differ so CueEngine resets its
// latches. Previously the caller passed only routeIdentifier, so a revision
// bump was invisible to the engine — ghost arrivingInM cues followed.
class RouteKeyTest {

    @Test
    fun returnsNullWhenIdentifierIsNull() {
        assertNull(buildRouteKey(null, 1))
        assertNull(buildRouteKey(null, null))
    }

    @Test
    fun includesBothIdentifierAndRevision() {
        assertEquals("r1-rev1", buildRouteKey("r1", 1))
        assertEquals("r1-rev2", buildRouteKey("r1", 2))
    }

    @Test
    fun differentRevisionProducesDifferentKey() {
        assertNotEquals(buildRouteKey("r1", 1), buildRouteKey("r1", 2))
    }

    @Test
    fun defaultsRevisionToZeroWhenNull() {
        assertEquals("r1-rev0", buildRouteKey("r1", null))
    }
}
