package me.fiksu.esp32map.companion.integration.cues

import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

class CueEngineTest {

    private fun mLeft(id: String, distance: Double) =
        CueManeuver(id, ManeuverKind.LEFT, distance)

    private fun base(
        routeId: String? = "r1",
        progressDistanceM: Double = 0.0,
        maneuvers: List<CueManeuver> = listOf(mLeft("m1", 200.0), mLeft("m2", 400.0)),
        offRoute: Boolean = false,
        rerouting: Boolean = false,
        arrived: Boolean = false,
        distanceFromRouteM: Double = 0.0,
        routeTotalDistanceM: Double = 1000.0,
        pairedWithDevice: Boolean = false,
    ) = CueSnapshot(
        routeId, pairedWithDevice, progressDistanceM, maneuvers,
        offRoute, rerouting, arrived, distanceFromRouteM, routeTotalDistanceM,
    )

    // First-tick announcement (replaces "Route started"). User-feedback:
    // hearing "Route started" on every Start was useless padding; the
    // rider needs the next-turn announcement instead.

    @Test
    fun firstTickAnnouncesNextTurnInsteadOfRouteStarted() {
        val r = CueEngine.tick(base(), CueEngineState())
        val firstTick = r.events.firstOrNull { it is CueEvent.NextTurnInAbout } as? CueEvent.NextTurnInAbout
        assertNotNull(firstTick)
        assertEquals(ManeuverKind.LEFT, firstTick!!.turnKind)
        assertEquals(200.0, firstTick.distanceM, 0.5)
    }

    @Test
    fun firstTickAnnouncementDoesNotRepeatAfterBackgroundGap() {
        // Coming back from a long backgrounded period must not re-fire
        // the start cue: the rider has been on the route for a while.
        val s1 = CueEngine.tick(base(), CueEngineState()).nextState
        val s2 = CueEngine.tick(base(progressDistanceM = 5.0), s1)
        assertNull(s2.events.firstOrNull { it is CueEvent.NextTurnInAbout })
    }

    @Test
    fun emitsTurn50mWhenCrossingThreshold() {
        val s1 = CueEngine.tick(base(progressDistanceM = 100.0), CueEngineState()).nextState
        val s2 = CueEngine.tick(base(progressDistanceM = 155.0), s1)
        val ev = s2.events.firstOrNull { it is CueEvent.Turn50m }
        assertNotNull(ev)
        assertEquals(ManeuverKind.LEFT, (ev as CueEvent.Turn50m).turnKind)
    }

    @Test
    fun doesNotReEmit50mOnSubsequentTicks() {
        val s1 = CueEngine.tick(base(progressDistanceM = 155.0), CueEngineState()).nextState
        val s2 = CueEngine.tick(base(progressDistanceM = 160.0), s1)
        assertNull(s2.events.firstOrNull { it is CueEvent.Turn50m })
    }

    @Test
    fun emitsTurn10mWhenCrossingThreshold() {
        val s1 = CueEngine.tick(base(progressDistanceM = 155.0), CueEngineState()).nextState
        val s2 = CueEngine.tick(base(progressDistanceM = 192.0), s1)
        val ev = s2.events.firstOrNull { it is CueEvent.Turn10m }
        assertNotNull(ev)
        assertEquals(ManeuverKind.LEFT, (ev as CueEvent.Turn10m).turnKind)
    }

    @Test
    fun emitsNextTurnInAboutAfterPassingManeuver() {
        val s1 = CueEngine.tick(base(progressDistanceM = 200.0), CueEngineState()).nextState
        val s2 = CueEngine.tick(base(progressDistanceM = 211.0), s1)
        val ev = s2.events.firstOrNull { it is CueEvent.NextTurnInAbout } as? CueEvent.NextTurnInAbout
        assertNotNull(ev)
        assertEquals(ManeuverKind.LEFT, ev!!.turnKind)
        assertEquals(189.0, ev.distanceM, 0.5)
    }

    @Test
    fun emitsArrivingInMWhenPastLastManeuverWithNoMore() {
        val snap = base(
            progressDistanceM = 412.0,
            maneuvers = listOf(mLeft("m1", 400.0)),
            routeTotalDistanceM = 600.0,
        )
        val r = CueEngine.tick(snap, CueEngineState())
        val ev = r.events.firstOrNull { it is CueEvent.ArrivingInM } as? CueEvent.ArrivingInM
        assertNotNull(ev)
        assertEquals(188.0, ev!!.distanceM, 0.5)
    }

    @Test
    fun emitsArrivedWhenArrivedFlagIsTrue() {
        val r = CueEngine.tick(base(arrived = true), CueEngineState())
        assertNotNull(r.events.firstOrNull { it is CueEvent.Arrived })
    }

    @Test
    fun emitsOffTrackOnFirstEpisodeRisingEdge() {
        val s1 = CueEngine.tick(base(), CueEngineState()).nextState
        val s2 = CueEngine.tick(base(offRoute = true, distanceFromRouteM = 40.0), s1)
        assertNotNull(s2.events.firstOrNull { it is CueEvent.OffTrack })
    }

    @Test
    fun emitsReroutingOnRisingEdge() {
        val s1 = CueEngine.tick(base(offRoute = true), CueEngineState()).nextState
        val s2 = CueEngine.tick(base(offRoute = true, rerouting = true), s1)
        assertNotNull(s2.events.firstOrNull { it is CueEvent.Rerouting })
    }

    @Test
    fun afterMoreThanTwoOffRouteEpisodesGoesSilent() {
        var s = CueEngineState()
        s = CueEngine.tick(base(offRoute = true, distanceFromRouteM = 40.0), s).nextState
        s = CueEngine.tick(base(offRoute = false, distanceFromRouteM = 5.0), s).nextState
        s = CueEngine.tick(base(offRoute = true, distanceFromRouteM = 40.0), s).nextState
        s = CueEngine.tick(base(offRoute = false, distanceFromRouteM = 5.0), s).nextState
        val r3 = CueEngine.tick(base(offRoute = true, distanceFromRouteM = 40.0), s)
        assertNotNull(r3.events.firstOrNull { it is CueEvent.RepeatedOffTrackSilence })
        s = r3.nextState
        // While silenced, no events fire even on threshold crossings.
        val r4 = CueEngine.tick(base(offRoute = true, progressDistanceM = 155.0), s)
        assertEquals(0, r4.events.size)
    }

    @Test
    fun emitsOnTrackAfterFiveConsecutiveOnRouteSamples() {
        var s = CueEngineState(
            lastRouteId = "r1",
            routeStartedAnnounced = true,
            offRouteEpisodeCount = 3,
            silenced = true,
            prevOffRoute = true,
        )
        repeat(4) {
            val r = CueEngine.tick(base(offRoute = false, distanceFromRouteM = 5.0), s)
            assertNull(r.events.firstOrNull { it is CueEvent.OnTrack })
            s = r.nextState
        }
        val r5 = CueEngine.tick(base(offRoute = false, distanceFromRouteM = 5.0), s)
        assertNotNull(r5.events.firstOrNull { it is CueEvent.OnTrack })
    }

    @Test
    fun pairedWithDeviceSuppressesAllCues() {
        val snap = base(
            pairedWithDevice = true,
            progressDistanceM = 192.0,
            offRoute = true,
        )
        val r = CueEngine.tick(snap, CueEngineState())
        assertEquals(0, r.events.size)
    }

    @Test
    fun resetsLatchesOnRouteIdChange() {
        val s1 = CueEngine.tick(base(progressDistanceM = 155.0), CueEngineState()).nextState
        val s2 = CueEngine.tick(base(routeId = "r2", progressDistanceM = 155.0), s1)
        assertNotNull(s2.events.firstOrNull { it is CueEvent.NextTurnInAbout })
    }

    @Test
    fun back2backTurnsCoalesceIntoCombinedCue() {
        val m1 = CueManeuver("m1", ManeuverKind.RIGHT, 200.0)
        val m2 = CueManeuver("m2", ManeuverKind.LEFT, 230.0)
        val s1 = CueEngine.tick(
            base(progressDistanceM = 100.0, maneuvers = listOf(m1, m2)),
            CueEngineState(),
        ).nextState
        val s2 = CueEngine.tick(
            base(progressDistanceM = 155.0, maneuvers = listOf(m1, m2)),
            s1,
        )
        val combined = s2.events.firstOrNull {
            it is CueEvent.Turn50m && it.followUpKind != null
        } as? CueEvent.Turn50m
        assertNotNull(combined)
        assertEquals(ManeuverKind.RIGHT, combined!!.turnKind)
        assertEquals(ManeuverKind.LEFT, combined.followUpKind)
    }

    // Bug 1: when the last cue maneuver sits within ~30 m of the route end,
    // the engine must emit arrivingInM instead of nextTurnInAbout / turn50m.

    @Test
    fun firstTick_lastManeuverCloseToDestination_emitsArrivingNotNextTurn() {
        // Single maneuver at 975 m; route ends at 1000 m (25 m gap < 30 m threshold).
        val snap = base(
            maneuvers = listOf(CueManeuver("m1", ManeuverKind.RIGHT, 975.0)),
            routeTotalDistanceM = 1000.0,
            progressDistanceM = 0.0,
        )
        val r = CueEngine.tick(snap, CueEngineState())
        assertNotNull("must emit arrivingInM", r.events.firstOrNull { it is CueEvent.ArrivingInM })
        assertNull("must not emit nextTurnInAbout", r.events.firstOrNull { it is CueEvent.NextTurnInAbout })
    }

    @Test
    fun firstTick_lastManeuverFarFromDestination_emitsNextTurnNotArriving() {
        // Same setup but maneuver at 200 m — clearly not close to the route end.
        val snap = base(
            maneuvers = listOf(CueManeuver("m1", ManeuverKind.RIGHT, 200.0)),
            routeTotalDistanceM = 1000.0,
            progressDistanceM = 0.0,
        )
        val r = CueEngine.tick(snap, CueEngineState())
        assertNotNull("must emit nextTurnInAbout", r.events.firstOrNull { it is CueEvent.NextTurnInAbout })
        assertNull("must not emit arrivingInM", r.events.firstOrNull { it is CueEvent.ArrivingInM })
    }

    @Test
    fun afterPassingBlock_lastManeuverCloseToDestination_emitsArrivingNotNextTurn() {
        // m1 at 400 m, m2 at 975 m of 1000 m; rider passes m1.
        val snap = base(
            maneuvers = listOf(
                CueManeuver("m1", ManeuverKind.LEFT, 400.0),
                CueManeuver("m2", ManeuverKind.RIGHT, 975.0),
            ),
            routeTotalDistanceM = 1000.0,
            progressDistanceM = 415.0,
        )
        val r = CueEngine.tick(CueEngineState(), snap)
        assertNotNull("must emit arrivingInM", r.events.firstOrNull { it is CueEvent.ArrivingInM })
        assertNull("must not emit nextTurnInAbout", r.events.firstOrNull { it is CueEvent.NextTurnInAbout })
    }

    @Test
    fun afterPassingBlock_lastManeuverCloseToDestination_suppressesTurn50m() {
        // Rider at 930 m; m2 at 975 m — within 50 m approach AND
        // last maneuver within 30 m of end. turn50m must not fire.
        val snap = base(
            maneuvers = listOf(
                CueManeuver("m1", ManeuverKind.LEFT, 400.0),
                CueManeuver("m2", ManeuverKind.RIGHT, 975.0),
            ),
            routeTotalDistanceM = 1000.0,
            progressDistanceM = 930.0,
        )
        val r = CueEngine.tick(CueEngineState(), snap)
        assertNull("turn50m must be suppressed for last maneuver near end", r.events.firstOrNull { it is CueEvent.Turn50m })
    }

    // Bug 3: same routeIdentifier with bumped revision must be detected as a
    // route change when the caller uses a composite "id-revN" key.

    @Test
    fun sameRouteIdWithBumpedRevisionCompositeKey_resetsEngineState() {
        // Simulate 300 m progress on route "lshape-rev1".
        val s1 = CueEngine.tick(
            base(routeId = "lshape-rev1", progressDistanceM = 300.0),
            CueEngineState(),
        ).nextState

        // Reroute: same base identifier, revision bumped ("lshape-rev2").
        val rerouteSnap = base(
            routeId = "lshape-rev2",
            progressDistanceM = 0.0,
            maneuvers = listOf(CueManeuver("r-m1", ManeuverKind.RIGHT, 100.0)),
            routeTotalDistanceM = 200.0,
        )
        val r = CueEngine.tick(rerouteSnap, s1)

        val arrivingCues = r.events.filterIsInstance<CueEvent.ArrivingInM>()
        assertTrue("must not fire ghost arrivingInM after reroute — got ${ r.events}", arrivingCues.isEmpty())
        val nextTurnCues = r.events.filterIsInstance<CueEvent.NextTurnInAbout>()
            .filter { it.turnKind == ManeuverKind.RIGHT }
        assertEquals("orientation cue must fire exactly once — got ${r.events}", 1, nextTurnCues.size)
    }

    @Test
    fun formatsSpecPhrases() {
        assertEquals("In 50 meters, turn left", CueEngine.format(CueEvent.Turn50m(ManeuverKind.LEFT)))
        assertEquals("In 50 meters, keep right", CueEngine.format(CueEvent.Turn50m(ManeuverKind.KEEP_RIGHT)))
        assertEquals("In 50 meters, take the left exit", CueEngine.format(CueEvent.Turn50m(ManeuverKind.EXIT_LEFT)))
        assertEquals("Turn right", CueEngine.format(CueEvent.Turn10m(ManeuverKind.RIGHT)))
        assertEquals(
            "Next turn left in about 190 meters",
            CueEngine.format(CueEvent.NextTurnInAbout(ManeuverKind.LEFT, 187.0)),
        )
        assertEquals(
            "Arriving at your destination in 180 meters",
            CueEngine.format(CueEvent.ArrivingInM(184.0)),
        )
        assertEquals("You have arrived at your destination", CueEngine.format(CueEvent.Arrived))
        assertEquals("Off track", CueEngine.format(CueEvent.OffTrack))
        assertEquals("Off track", CueEngine.format(CueEvent.RepeatedOffTrackSilence))
        assertEquals("Rerouting", CueEngine.format(CueEvent.Rerouting))
        assertEquals("On track", CueEngine.format(CueEvent.OnTrack))
    }
}
