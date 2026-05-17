package app.navon.bike.integration.cues

import app.navon.bike.domain.RouteManeuverType
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

/**
 * Filter contract for [CueManeuverMapping.kindFor] — the single point of
 * truth that decides which [RouteManeuverType] values produce an audio
 * cue and which are silenced.
 *
 * Bugs covered:
 *  - Bug 1: [RouteManeuverType.STRAIGHT] must produce no cue (otherwise
 *    the engine fires "Next turn in about X meters" / "Follow the route"
 *    for a non-turn).
 *  - Bug 2: [RouteManeuverType.ROUNDABOUT] / [RouteManeuverType.MERGE] /
 *    [RouteManeuverType.RAMP] must map to first-class kinds, not GENERIC.
 *  - Bug 3: no [RouteManeuverType] may produce KEEP_LEFT or KEEP_RIGHT;
 *    those enum cases were removed from [ManeuverKind] entirely. Kotlin's
 *    `when` exhaustiveness check is the contract enforcer.
 */
class CueManeuverFilterTest {

    // Bug 1: straight is not a turn

    @Test
    fun straight_producesNoCue() {
        assertNull(CueManeuverMapping.kindFor(RouteManeuverType.STRAIGHT))
    }

    // Bug 2: first-class roundabout / merge / ramp

    @Test
    fun roundabout_producesRoundaboutKind() {
        assertEquals(ManeuverKind.ROUNDABOUT, CueManeuverMapping.kindFor(RouteManeuverType.ROUNDABOUT))
    }

    @Test
    fun merge_producesMergeKind() {
        assertEquals(ManeuverKind.MERGE, CueManeuverMapping.kindFor(RouteManeuverType.MERGE))
    }

    @Test
    fun ramp_producesRampKind() {
        assertEquals(ManeuverKind.RAMP, CueManeuverMapping.kindFor(RouteManeuverType.RAMP))
    }

    // Existing silence-by-design contract (regression guards)

    @Test
    fun slightLeft_mapsToSlightLeftKind() {
        assertEquals(ManeuverKind.SLIGHT_LEFT, CueManeuverMapping.kindFor(RouteManeuverType.SLIGHT_LEFT))
        // isMinorKeepFor removed — slightLeft is now first-class
    }

    @Test
    fun slightRight_mapsToSlightRightKind() {
        assertEquals(ManeuverKind.SLIGHT_RIGHT, CueManeuverMapping.kindFor(RouteManeuverType.SLIGHT_RIGHT))
        // isMinorKeepFor removed — slightRight is now first-class
    }

    @Test
    fun depart_producesNoCue() {
        assertNull(CueManeuverMapping.kindFor(RouteManeuverType.DEPART))
    }

    @Test
    fun arrive_producesNoCue() {
        assertNull(CueManeuverMapping.kindFor(RouteManeuverType.ARRIVE))
    }

    // Real turns still fire

    @Test
    fun left_producesLeftKind() {
        assertEquals(ManeuverKind.LEFT, CueManeuverMapping.kindFor(RouteManeuverType.LEFT))
    }

    @Test
    fun sharpLeft_producesLeftKind() {
        assertEquals(ManeuverKind.LEFT, CueManeuverMapping.kindFor(RouteManeuverType.SHARP_LEFT))
    }

    @Test
    fun right_producesRightKind() {
        assertEquals(ManeuverKind.RIGHT, CueManeuverMapping.kindFor(RouteManeuverType.RIGHT))
    }

    @Test
    fun sharpRight_producesRightKind() {
        assertEquals(ManeuverKind.RIGHT, CueManeuverMapping.kindFor(RouteManeuverType.SHARP_RIGHT))
    }

    @Test
    fun uturn_producesUturnKind() {
        assertEquals(ManeuverKind.UTURN, CueManeuverMapping.kindFor(RouteManeuverType.UTURN))
    }
}
