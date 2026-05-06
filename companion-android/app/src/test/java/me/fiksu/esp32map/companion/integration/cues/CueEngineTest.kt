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
        val s1 = CueEngine.tick(base(progressDistanceM = 100.0), CueEngineState()).nextState
        val s2 = CueEngine.tick(base(routeId = "r2", progressDistanceM = 100.0), s1)
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
        val r = CueEngine.tick(snap, CueEngineState())
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
        val r = CueEngine.tick(snap, CueEngineState())
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
        assertEquals("In 50 meters, turn left", CueEngine.format(CueEvent.Turn50m(ManeuverKind.LEFT, 50.0)))
        assertEquals("In 50 meters, take the left exit", CueEngine.format(CueEvent.Turn50m(ManeuverKind.EXIT_LEFT, 50.0)))
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

    // ─── turn10m fires 15m before the maneuver (5m earlier) ──────────────────

    @Test
    fun turn10mFiresAt14m_withinNew15mThreshold() {
        val s1 = CueEngine.tick(base(progressDistanceM = 100.0), CueEngineState()).nextState
        // 200 - 186 = 14 m remaining — fires with the new 15m threshold
        val r2 = CueEngine.tick(base(progressDistanceM = 186.0), s1)
        assertNotNull(
            "turn10m must fire when 14m before the turn (new 15m threshold)",
            r2.events.firstOrNull { it is CueEvent.Turn10m },
        )
    }

    @Test
    fun turn10mDoesNotFireAt16m_outsideThreshold() {
        val s1 = CueEngine.tick(base(progressDistanceM = 100.0), CueEngineState()).nextState
        // 200 - 184 = 16 m remaining — must NOT fire
        val r2 = CueEngine.tick(base(progressDistanceM = 184.0), s1)
        assertNull(
            "turn10m must not fire when 16m before the turn (outside 15m threshold)",
            r2.events.firstOrNull { it is CueEvent.Turn10m },
        )
    }

    // ─── skip turn50m when next turn is < 100m away ───────────────────────────

    @Test
    fun skipsTurn50m_whenRouteStarts80mFromFirstTurn() {
        // 80m > 50m → Case A fires nextTurnInAbout; but 80m < 100m so
        // turn50m must be pre-latched and must not fire when rider enters 50m window.
        val m = CueManeuver("m1", ManeuverKind.LEFT, 80.0)
        val snap = base(maneuvers = listOf(m), routeTotalDistanceM = 500.0)
        val s1 = CueEngine.tick(snap, CueEngineState())
        assertNotNull("nextTurnInAbout must fire", s1.events.firstOrNull { it is CueEvent.NextTurnInAbout })
        // Advance into the 50m window (80-35=45m) — turn50m must NOT fire
        val s2 = CueEngine.tick(snap.copy(progressDistanceM = 35.0), s1.nextState)
        assertNull("turn50m must not fire when first turn was < 100m at route start",
            s2.events.firstOrNull { it is CueEvent.Turn50m })
    }

    @Test
    fun firesTurn50m_whenRouteStarts120mFromFirstTurn() {
        // 120m > 100m → NOT pre-latched, turn50m fires normally
        val m = CueManeuver("m1", ManeuverKind.LEFT, 120.0)
        val snap = base(maneuvers = listOf(m), routeTotalDistanceM = 500.0)
        val s1 = CueEngine.tick(snap, CueEngineState()).nextState
        // 120-75=45m → inside 50m window
        val r2 = CueEngine.tick(snap.copy(progressDistanceM = 75.0), s1)
        assertNotNull("turn50m must fire when turn was >= 100m at route start",
            r2.events.firstOrNull { it is CueEvent.Turn50m })
    }

    @Test
    fun skipsTurn50m_afterPassingManeuver_whenNextIs60mAway() {
        // m1 at 100m, m2 at 170m. After passing m1 (rider at 110m), m2 is 60m away (<100m).
        val m1 = CueManeuver("m1", ManeuverKind.LEFT, 100.0)
        val m2 = CueManeuver("m2", ManeuverKind.LEFT, 170.0)
        val maneuvers = listOf(m1, m2)
        val s1 = CueEngine.tick(base(maneuvers = maneuvers), CueEngineState()).nextState
        val s2 = CueEngine.tick(base(progressDistanceM = 110.0, maneuvers = maneuvers), s1)
        assertNotNull("nextTurnInAbout must fire after passing m1",
            s2.events.firstOrNull { it is CueEvent.NextTurnInAbout })
        // Advance into m2's 50m window (170-125=45m) — turn50m must NOT fire
        val s3 = CueEngine.tick(base(progressDistanceM = 125.0, maneuvers = maneuvers), s2.nextState)
        assertNull("turn50m must not fire when next turn was < 100m at time of nextTurnInAbout",
            s3.events.firstOrNull { it is CueEvent.Turn50m })
    }

    // ─── first-class roundabout / merge / ramp cues (bug 2) ──────────────────

    @Test
    fun formatsRoundaboutPhrases() {
        assertEquals(
            "In 50 meters, enter the roundabout",
            CueEngine.format(CueEvent.Turn50m(ManeuverKind.ROUNDABOUT, 50.0)),
        )
        assertEquals("Enter the roundabout", CueEngine.format(CueEvent.Turn10m(ManeuverKind.ROUNDABOUT)))
        assertEquals(
            "Next roundabout in about 200 meters",
            CueEngine.format(CueEvent.NextTurnInAbout(ManeuverKind.ROUNDABOUT, 200.0)),
        )
    }

    @Test
    fun formatsMergePhrases() {
        assertEquals(
            "In 50 meters, merge",
            CueEngine.format(CueEvent.Turn50m(ManeuverKind.MERGE, 50.0)),
        )
        assertEquals("Merge", CueEngine.format(CueEvent.Turn10m(ManeuverKind.MERGE)))
        assertEquals(
            "Next merge in about 200 meters",
            CueEngine.format(CueEvent.NextTurnInAbout(ManeuverKind.MERGE, 200.0)),
        )
    }

    @Test
    fun formatsRampPhrases() {
        assertEquals(
            "In 50 meters, take the ramp",
            CueEngine.format(CueEvent.Turn50m(ManeuverKind.RAMP, 50.0)),
        )
        assertEquals("Take the ramp", CueEngine.format(CueEvent.Turn10m(ManeuverKind.RAMP)))
        assertEquals(
            "Next ramp in about 200 meters",
            CueEngine.format(CueEvent.NextTurnInAbout(ManeuverKind.RAMP, 200.0)),
        )
    }

    // ─── bug 4: arrived flag suppresses arrivingInM same tick ────────────────

    @Test
    fun arrivingInM_suppressedWhenArrivedFlagTrueSameTick() {
        // After-passing block would fire ArrivingInM (rider passed the only
        // maneuver, no follow-up). On the same tick mark arrived = true —
        // only the dedicated `Arrived` cue should fire, not both.
        val m1 = CueManeuver("m1", ManeuverKind.LEFT, 400.0)
        var s = CueEngine.tick(
            base(progressDistanceM = 100.0, maneuvers = listOf(m1), routeTotalDistanceM = 600.0),
            CueEngineState(),
        ).nextState
        val r = CueEngine.tick(
            base(
                progressDistanceM = 595.0,
                maneuvers = listOf(m1),
                arrived = true,
                routeTotalDistanceM = 600.0,
            ),
            s,
        )
        assertNull(
            "arrivingInM must be suppressed when arrived flag is true on the same tick — got ${r.events}",
            r.events.firstOrNull { it is CueEvent.ArrivingInM },
        )
        assertNotNull(
            "arrived cue must fire — got ${r.events}",
            r.events.firstOrNull { it is CueEvent.Arrived },
        )
    }

    // ─── bug 5: back-to-back pair under 15m at route start emits combined cue ─

    @Test
    fun firstTick_backToBackPair_under15mFromStart_emitsCombinedCue() {
        // Route starts 10m before m1; m2 follows 15m later (within 30m
        // back-to-back threshold). Without the fix, the regular 50m block
        // gates on d > 15m and never fires — only `Turn10m(m1)` plays and
        // the rider misses m2.
        val m1 = CueManeuver("m1", ManeuverKind.RIGHT, 10.0)
        val m2 = CueManeuver("m2", ManeuverKind.LEFT, 25.0)
        val r = CueEngine.tick(
            base(progressDistanceM = 0.0, maneuvers = listOf(m1, m2), routeTotalDistanceM = 1000.0),
            CueEngineState(),
        )
        val combined = r.events.firstOrNull {
            it is CueEvent.Turn50m && it.followUpKind != null
        } as? CueEvent.Turn50m
        assertNotNull(
            "first-tick back-to-back pair under 15m must emit combined Turn50m cue — got ${r.events}",
            combined,
        )
        assertEquals(ManeuverKind.RIGHT, combined!!.turnKind)
        assertEquals(ManeuverKind.LEFT, combined.followUpKind)
        assertEquals(10.0, combined.distanceM, 0.5)
    }

    // ─── back-to-back fusion at the 10 m tier (sparse-GPS escape hatch) ─────

    // User-reported regression: M1 (right) ~10 m before M2 (left). The rider
    // heard "turn right" only — no warning of the immediate left after. The
    // 50 m fusion branch is gated on `d > APPROACH_10_M`, so when a tick
    // first lands inside the 10 m approach window (sparse GPS, fast cycling,
    // app foregrounded mid-ride), the 50 m combined cue is skipped and the
    // unsuffixed Turn10m fires alone. The 10 m branch needs the same
    // back-to-back peek the 50 m branch already performs.
    @Test
    fun turn10m_coalescesFollowUp_whenFirstInRangeTickIsAlreadyInside15m() {
        val m1 = CueManeuver("m1", ManeuverKind.RIGHT, 200.0)
        val m2 = CueManeuver("m2", ManeuverKind.LEFT, 210.0)
        // Tick 1 at 50 m progress — orientation cue path.
        val s1 = CueEngine.tick(
            base(progressDistanceM = 50.0, maneuvers = listOf(m1, m2)),
            CueEngineState(),
        ).nextState
        // Sparse-GPS jump from 50 m to 190 m progress — 10 m before m1, so
        // the 50 m approach window was skipped entirely.
        val r2 = CueEngine.tick(
            base(progressDistanceM = 190.0, maneuvers = listOf(m1, m2)),
            s1,
        )
        val combined = r2.events.firstOrNull {
            it is CueEvent.Turn10m && it.followUpKind != null
        } as? CueEvent.Turn10m
        assertNotNull(
            "Turn10m must coalesce with the follow-up when the 50 m window was skipped — got ${r2.events}",
            combined,
        )
        assertEquals(ManeuverKind.RIGHT, combined!!.turnKind)
        assertEquals(ManeuverKind.LEFT, combined.followUpKind)
    }

    // ─── rerouting cue silences after 2 episodes (across route id changes) ───

    private fun reroutingSnap(rerouting: Boolean, routeId: String = "r1") = CueSnapshot(
        routeId, false, 0.0,
        listOf(CueManeuver("m1", ManeuverKind.LEFT, 200.0)),
        false, rerouting, false, 0.0, 1000.0,
    )

    @Test
    fun rerouting_firesOn1stEpisode() {
        val s1 = CueEngine.tick(reroutingSnap(false), CueEngineState()).nextState
        val r2 = CueEngine.tick(reroutingSnap(true), s1)
        assertNotNull("1st rerouting episode must fire the cue",
            r2.events.firstOrNull { it is CueEvent.Rerouting })
    }

    @Test
    fun rerouting_firesOn2ndEpisode_acrossRouteIdChange() {
        var s = CueEngineState()
        s = CueEngine.tick(reroutingSnap(false, "r1"), s).nextState
        s = CueEngine.tick(reroutingSnap(true, "r1"), s).nextState  // 1st
        s = CueEngine.tick(reroutingSnap(false, "r2"), s).nextState // new route
        val r = CueEngine.tick(reroutingSnap(true, "r2"), s)        // 2nd
        assertNotNull("2nd rerouting episode must still fire — cap is 2",
            r.events.firstOrNull { it is CueEvent.Rerouting })
    }

    @Test
    fun rerouting_silencedOn3rdEpisode() {
        var s = CueEngineState()
        s = CueEngine.tick(reroutingSnap(false, "r1"), s).nextState
        s = CueEngine.tick(reroutingSnap(true, "r1"), s).nextState  // 1st
        s = CueEngine.tick(reroutingSnap(false, "r2"), s).nextState
        s = CueEngine.tick(reroutingSnap(true, "r2"), s).nextState  // 2nd
        s = CueEngine.tick(reroutingSnap(false, "r3"), s).nextState
        val r = CueEngine.tick(reroutingSnap(true, "r3"), s)        // 3rd
        assertNull("3rd rerouting episode must be silenced",
            r.events.firstOrNull { it is CueEvent.Rerouting })
    }

    @Test
    fun rerouting_resetsAfterConfirmedOnTrack() {
        var s = CueEngineState()
        s = CueEngine.tick(reroutingSnap(false, "r1"), s).nextState
        s = CueEngine.tick(reroutingSnap(true, "r1"), s).nextState
        s = CueEngine.tick(reroutingSnap(false, "r2"), s).nextState
        s = CueEngine.tick(reroutingSnap(true, "r2"), s).nextState
        repeat(5) {
            s = CueEngine.tick(reroutingSnap(false, "r2"), s).nextState
        }
        val r = CueEngine.tick(reroutingSnap(true, "r3"), s)
        assertNotNull("After confirmed on-track the rerouting cue counter must reset",
            r.events.firstOrNull { it is CueEvent.Rerouting })
    }
}
