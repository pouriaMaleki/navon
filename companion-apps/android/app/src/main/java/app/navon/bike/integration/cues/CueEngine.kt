package app.navon.bike.integration.cues

/**
 * Audio-cue trigger engine. Pure data-in / data-out — consumed by the
 * wiring layer which assembles a [CueSnapshot] each tick and routes the
 * resulting [CueEvent]s to a TTS service.
 *
 * Spec: docs/ux-specs.md lines 133-143.
 */

enum class ManeuverKind {
    LEFT,
    RIGHT,
    BEAR_LEFT,
    BEAR_RIGHT,
    EXIT_LEFT,
    EXIT_RIGHT,
    UTURN,
    ROUNDABOUT,
    MERGE,
    RAMP,
    GENERIC;
}

data class CueManeuver(
    val id: String,
    val kind: ManeuverKind,
    val distanceFromStartM: Double,
    val isMinorKeep: Boolean = false,
)

data class CueSnapshot(
    val routeId: String?,
    val pairedWithDevice: Boolean,
    val progressDistanceM: Double,
    /** Excluding depart/arrive; ordered by distanceFromStartM ascending. */
    val maneuvers: List<CueManeuver>,
    val offRoute: Boolean,
    val rerouting: Boolean,
    val arrived: Boolean,
    val distanceFromRouteM: Double,
    val routeTotalDistanceM: Double,
)

sealed class CueEvent {
    /** Approaching a turn (within ~50m). `distanceM` is the rider's
     *  actual distance to the maneuver — the catalog uses it instead
     *  of hardcoding "50 meters", because the rider can enter the
     *  approach window with d much smaller (route starting close to
     *  the turn) and "in 50 meters turn left" while actually 15 m
     *  away is jarringly inaccurate. When the next maneuver is within
     *  ~30 m of this one (a back-to-back pair), `followUpKind` carries
     *  that maneuver's direction so the engine emits a single combined
     *  cue ("In 20 meters, turn right then quickly left") instead of
     *  two overlapping cue chains. */
    data class Turn50m(
        val turnKind: ManeuverKind,
        val distanceM: Double = 50.0,
        val followUpKind: ManeuverKind? = null,
    ) : CueEvent()
    /** Immediate-action 10 m cue. `followUpKind` is set when the next
     *  maneuver is within `BACK_TO_BACK_THRESHOLD_M` of this one — covers
     *  the sparse-GPS / fast-cycling case where the 50 m combined cue was
     *  missed because the first in-range tick already landed inside 15 m
     *  of M1. Without this fold, the rider hears only "turn <first>" with
     *  no mention of the immediately-following turn. */
    data class Turn10m(
        val turnKind: ManeuverKind,
        val followUpKind: ManeuverKind? = null,
    ) : CueEvent()
    data class NextTurnInAbout(val turnKind: ManeuverKind, val distanceM: Double) : CueEvent()
    /** Single range-hold cue for bearLeft/bearRight. */
    data class BearRange(val turnKind: ManeuverKind, val distanceM: Double) : CueEvent()
    data class ArrivingInM(val distanceM: Double) : CueEvent()
    object Arrived : CueEvent()
    object OffTrack : CueEvent()
    object Rerouting : CueEvent()
    object RepeatedOffTrackSilence : CueEvent()
    object OnTrack : CueEvent()
}

data class CueEngineState(
    val lastRouteId: String? = null,
    val routeStartedAnnounced: Boolean = false,
    val announced50m: Set<String> = emptySet(),
    val announced10m: Set<String> = emptySet(),
    val announcedNextTurnAfter: Set<String> = emptySet(),
    val approachingDestinationAnnounced: Boolean = false,
    val arrivedAnnounced: Boolean = false,
    val offRouteEpisodeCount: Int = 0,
    val prevOffRoute: Boolean = false,
    val prevRerouting: Boolean = false,
    val silenced: Boolean = false,
    val consecutiveOnRouteSamples: Int = 0,
    val onTrackAnnounced: Boolean = false,
    /** Number of rerouting cues that have fired (counts rising edges).
     *  Persists across route id changes — every successful reroute issues a
     *  new route id, so resetting on route id would defeat the cue cap.
     *  Reset only when the rider is confirmed on-track for
     *  ON_TRACK_CONFIRM_SAMPLES consecutive ticks. */
    val reroutingEpisodeCount: Int = 0,
    /** Number of consecutive off-route ticks. Resets on any on-route tick.
     *  OffTrack only fires after OFF_ROUTE_HYSTERESIS_TICKS consecutive. */
    val offRouteTickCount: Int = 0,
)

object CueEngine {
    private const val APPROACH_50_M = 50.0
    private const val APPROACH_10_M = 15.0
    /** When the next turn is closer than this at the time of the NextTurnInAbout
     *  announcement, pre-latch Turn50m so it never fires. The rider has already
     *  been told the turn is near; a redundant "in 50 m" before they can react
     *  is jarring. */
    private const val SKIP_50M_BELOW_DISTANCE_M = 100.0
    private const val PASSED_TURN_M = 10.0
    private const val ON_TRACK_CONFIRM_SAMPLES = 5
    private const val ON_TRACK_CORRIDOR_M = 22.0
    /** Require this many consecutive off-route ticks before firing OffTrack.
     *  Prevents momentary GPS blips from triggering false off-track alerts. */
    private const val OFF_ROUTE_HYSTERESIS_TICKS = 3
    /** When the rider is this far from the route, skip hysteresis — they're genuinely lost. */
    private const val OFF_ROUTE_IMMEDIATE_DISTANCE_M = 50.0
    private const val REPEAT_OFFTRACK_SILENCE_THRESHOLD = 2
    /** Cap on rerouting audio cues per "off-route session". After this many
     *  fires, stay silent until the rider is confirmed on-track. */
    private const val REROUTING_CUE_CAP = 2
    /** Two maneuvers separated by less than this fold into a single
     *  "turn X then quickly Y" cue. Mirrors web's BACK_TO_BACK_THRESHOLD_M.
     *  30 m matches the spec phrase "then quickly" — at cycling speeds
     *  that's ~4-7 s apart, the only window where coalescing two turns
     *  into one cue actually feels natural. 50 m / 80 m both let
     *  genuinely separate maneuvers ride along on a combined cue, which
     *  the rider then misperceives as the routing engine inventing
     *  turns that aren't really there. */
    private const val BACK_TO_BACK_THRESHOLD_M = 30.0
    /** If the last cue maneuver sits within this distance of the route end,
     *  approaching it is indistinguishable from arriving: substitute ArrivingInM
     *  for any NextTurnInAbout or approach cues so the rider hears "arriving in Xm"
     *  rather than a phantom turn command. */
    /** Bear range cues are only useful when the bear segment is long enough.
     *  Below this threshold the rider is already there — silence the bear. */
    private const val MIN_BEAR_SEGMENT_M = 50.0
    private const val CLOSE_TO_DESTINATION_M = 30.0

    private fun isBearKind(kind: ManeuverKind): Boolean =
        kind == ManeuverKind.BEAR_LEFT || kind == ManeuverKind.BEAR_RIGHT

    data class Result(val events: List<CueEvent>, val nextState: CueEngineState)

    fun tick(snapshot: CueSnapshot, state: CueEngineState): Result {
        if (snapshot.pairedWithDevice) return Result(emptyList(), state)

        var s: CueEngineState = if (snapshot.routeId != state.lastRouteId) {
            // Persist rerouting silence across route id changes — every
            // successful reroute issues a new route id, so resetting here
            // would defeat the cue cap.
            CueEngineState(
                lastRouteId = snapshot.routeId,
                reroutingEpisodeCount = state.reroutingEpisodeCount,
            )
        } else state

        val events = mutableListOf<CueEvent>()

        // First-tick announcement (replaces "Route started"). User-feedback:
        // "Route started" was useless padding. Replace with the actual
        // next-turn announcement so the rider hears something immediately
        // useful on Start.
        //
        // Three sub-cases on this tick when the route just started:
        //   A) First turn is FAR (> 50 m): emit `NextTurnInAbout` as an
        //      orientation cue ("Next turn left in about 200 meters").
        //   B) First turn is IMMINENT and stands alone (no back-to-back
        //      follow-up within ~30 m): SKIP every announce; pre-latch
        //      the 50 m cue so only the 10 m approach cue speaks when
        //      the rider actually reaches the turn. User feedback: a
        //      route starting 15 m from a turn used to fire next-turn
        //      + 50 m + 10 m back-to-back — three cues for one turn,
        //      with disagreeing distances.
        //   C) First turn is IMMINENT and has a back-to-back companion
        //      within ~30 m: skip the orientation cue, let the 50 m
        //      block emit the combined "in X meters turn left then
        //      quickly right" cue with the ACTUAL distance. That's the
        //      only way to warn the rider about TWO close turns in one
        //      breath, so it stays.
        if (snapshot.routeId != null && !s.routeStartedAnnounced) {
            val firstNonDepart = snapshot.maneuvers.firstOrNull {
                it.distanceFromStartM - snapshot.progressDistanceM >= 0
            }
            var nextAnnounced50m: Set<String> = s.announced50m
            if (firstNonDepart != null) {
                val distanceM = firstNonDepart.distanceFromStartM - snapshot.progressDistanceM
                if (distanceM > APPROACH_50_M) {
                    // Case A — orientation cue.
                    // Bug 1: if firstNonDepart is the last cue maneuver AND very close
                    // to the route end, announce "arriving" instead of a phantom turn.
                    val firstIdx = snapshot.maneuvers.indexOfFirst { it.id == firstNonDepart.id }
                    val isLastManeuver = firstIdx == snapshot.maneuvers.size - 1
                    val distToEnd = snapshot.routeTotalDistanceM - firstNonDepart.distanceFromStartM
                    if (isLastManeuver && distToEnd < CLOSE_TO_DESTINATION_M) {
                        if (!s.approachingDestinationAnnounced) {
                            events.add(CueEvent.ArrivingInM(
                                distanceM = snapshot.routeTotalDistanceM - snapshot.progressDistanceM,
                            ))
                            s = s.copy(
                                approachingDestinationAnnounced = true,
                                announced50m = s.announced50m + firstNonDepart.id,
                                announced10m = s.announced10m + firstNonDepart.id,
                            )
                        }
                    } else if (!firstNonDepart.isMinorKeep && !isBearKind(firstNonDepart.kind)) {
                        events.add(
                            CueEvent.NextTurnInAbout(
                                turnKind = firstNonDepart.kind,
                                distanceM = distanceM,
                            ),
                        )
                        // Pre-latch Turn50m when the first turn is already close: the
                        // rider has the orientation cue; a redundant "in 50 m" a few
                        // seconds later would be jarring before they can even react.
                        if (distanceM < SKIP_50M_BELOW_DISTANCE_M) {
                            nextAnnounced50m = nextAnnounced50m + firstNonDepart.id
                        }
                    }
                } else {
                    // Case B vs. C — peek at the follow-up gap.
                    val upcomingIdx = snapshot.maneuvers.indexOfFirst { it.id == firstNonDepart.id }
                    val follow = snapshot.maneuvers.getOrNull(upcomingIdx + 1)
                    val gap = follow?.let { it.distanceFromStartM - firstNonDepart.distanceFromStartM }
                        ?: Double.POSITIVE_INFINITY
                    if (follow != null && gap <= BACK_TO_BACK_THRESHOLD_M) {
                        // Case C: emit the combined cue here directly
                        // with the actual distance. The regular 50 m
                        // block downstream gates on `d > APPROACH_10_M`
                        // (15 m) and would skip routes starting < 15 m
                        // before a back-to-back pair, leaving the rider
                        // with only `Turn10m(first)` and no warning
                        // about the second turn.
                        if (!firstNonDepart.isMinorKeep && !isBearKind(firstNonDepart.kind)) {
                            events.add(
                                CueEvent.Turn50m(
                                    turnKind = firstNonDepart.kind,
                                    distanceM = distanceM,
                                    followUpKind = follow.kind,
                                ),
                            )
                        }
                        nextAnnounced50m = nextAnnounced50m + firstNonDepart.id + follow.id
                        s = s.copy(
                            announced10m = s.announced10m + firstNonDepart.id + follow.id,
                            announcedNextTurnAfter = s.announcedNextTurnAfter + firstNonDepart.id,
                        )
                    } else {
                        // Case B: pre-latch the 50 m cue so only the
                        // 10 m action cue fires for this maneuver.
                        nextAnnounced50m = nextAnnounced50m + firstNonDepart.id
                    }
                }
            }
            s = s.copy(routeStartedAnnounced = true, announced50m = nextAnnounced50m)
        }

        // Off-route hysteresis: count consecutive off-route ticks. Fire
        // OffTrack only after OFF_ROUTE_HYSTERESIS_TICKS consecutive, or
        // immediately when the rider is far from the route (genuinely lost,
        // not a GPS blip).
        var offRouteTickCount = s.offRouteTickCount
        if (snapshot.offRoute) {
            offRouteTickCount += 1
        } else {
            offRouteTickCount = 0
        }

        var offRouteEpisodeCount = s.offRouteEpisodeCount

        val immediateOffTrack =
            snapshot.offRoute &&
            snapshot.distanceFromRouteM > OFF_ROUTE_IMMEDIATE_DISTANCE_M &&
            offRouteTickCount == 1
        val hysteresisOffTrack =
            snapshot.offRoute && offRouteTickCount == OFF_ROUTE_HYSTERESIS_TICKS
        val offTrackFired = immediateOffTrack || hysteresisOffTrack

        if (offTrackFired) offRouteEpisodeCount += 1

        var silenced = s.silenced
        var onTrackAnnounced = s.onTrackAnnounced
        var consecutiveOnRouteSamples = s.consecutiveOnRouteSamples
        var reroutingEpisodeCount = s.reroutingEpisodeCount

        if (!snapshot.offRoute && snapshot.distanceFromRouteM < ON_TRACK_CORRIDOR_M) {
            consecutiveOnRouteSamples += 1
        } else {
            consecutiveOnRouteSamples = 0
        }

        if (silenced && consecutiveOnRouteSamples >= ON_TRACK_CONFIRM_SAMPLES && !onTrackAnnounced) {
            events.add(CueEvent.OnTrack)
            silenced = false
            onTrackAnnounced = true
            offRouteEpisodeCount = 0
        }
        if (consecutiveOnRouteSamples >= ON_TRACK_CONFIRM_SAMPLES) {
            reroutingEpisodeCount = 0
        }

        if (offTrackFired && offRouteEpisodeCount > REPEAT_OFFTRACK_SILENCE_THRESHOLD && !silenced) {
            events.add(CueEvent.RepeatedOffTrackSilence)
            silenced = true
            onTrackAnnounced = false
        } else if (offTrackFired && !silenced) {
            events.add(CueEvent.OffTrack)
        }

        // Rerouting rising edge — capped at REROUTING_CUE_CAP per off-route session.
        val reroutingRose = !s.prevRerouting && snapshot.rerouting
        if (reroutingRose) reroutingEpisodeCount += 1
        if (reroutingRose && !silenced && reroutingEpisodeCount <= REROUTING_CUE_CAP) {
            events.add(CueEvent.Rerouting)
        }

        if (!silenced && !snapshot.offRoute && !snapshot.rerouting) {
            val announced50m = s.announced50m.toMutableSet()
            val announced10m = s.announced10m.toMutableSet()
            val announcedNextTurnAfter = s.announcedNextTurnAfter.toMutableSet()

            val upcoming = snapshot.maneuvers.firstOrNull {
                it.distanceFromStartM - snapshot.progressDistanceM >= 0
            }
            val upcomingDistance = upcoming?.let { it.distanceFromStartM - snapshot.progressDistanceM }

            if (upcoming != null && upcomingDistance != null &&
                upcomingDistance <= APPROACH_50_M && upcomingDistance > APPROACH_10_M &&
                !announced50m.contains(upcoming.id) && !upcoming.isMinorKeep && !isBearKind(upcoming.kind)
            ) {
                val upcomingIdx = snapshot.maneuvers.indexOfFirst { it.id == upcoming.id }
                val followUp = snapshot.maneuvers.getOrNull(upcomingIdx + 1)
                val gap = followUp?.let { it.distanceFromStartM - upcoming.distanceFromStartM }
                    ?: Double.POSITIVE_INFINITY
                if (followUp != null && gap <= BACK_TO_BACK_THRESHOLD_M) {
                    events.add(
                        CueEvent.Turn50m(
                            turnKind = upcoming.kind,
                            distanceM = upcomingDistance,
                            followUpKind = followUp.kind,
                        ),
                    )
                    announced50m.add(upcoming.id)
                    announced50m.add(followUp.id)
                    announced10m.add(followUp.id)
                    announcedNextTurnAfter.add(upcoming.id)
                } else {
                    events.add(
                        CueEvent.Turn50m(
                            turnKind = upcoming.kind,
                            distanceM = upcomingDistance,
                            followUpKind = null,
                        ),
                    )
                    announced50m.add(upcoming.id)
                }
            }
            if (upcoming != null && upcomingDistance != null &&
                upcomingDistance <= APPROACH_10_M && !announced10m.contains(upcoming.id) &&
                !upcoming.isMinorKeep && !isBearKind(upcoming.kind)
            ) {
                val upcomingIdx = snapshot.maneuvers.indexOfFirst { it.id == upcoming.id }
                val followUp = snapshot.maneuvers.getOrNull(upcomingIdx + 1)
                val gap = followUp?.let { it.distanceFromStartM - upcoming.distanceFromStartM }
                    ?: Double.POSITIVE_INFINITY
                if (followUp != null && gap <= BACK_TO_BACK_THRESHOLD_M) {
                    events.add(
                        CueEvent.Turn10m(
                            turnKind = upcoming.kind,
                            followUpKind = followUp.kind,
                        ),
                    )
                    announced10m.add(upcoming.id)
                    announced10m.add(followUp.id)
                    announced50m.add(followUp.id)
                    announcedNextTurnAfter.add(upcoming.id)
                } else {
                    events.add(CueEvent.Turn10m(upcoming.kind))
                    announced10m.add(upcoming.id)
                }
            }

            // Bear-range cue: for promoted slightLeft/slightRight, fire
            // a single range-hold cue when the rider enters the segment.
            if (upcoming != null && upcomingDistance != null &&
                isBearKind(upcoming.kind) &&
                upcomingDistance <= APPROACH_10_M &&
                !announced10m.contains(upcoming.id)
            ) {
                val upcomingIdx = snapshot.maneuvers.indexOfFirst { it.id == upcoming.id }
                val nextManeuver = snapshot.maneuvers.getOrNull(upcomingIdx + 1)
                val rawSegmentLengthM = nextManeuver?.let {
                    it.distanceFromStartM - upcoming.distanceFromStartM
                } ?: (snapshot.routeTotalDistanceM - upcoming.distanceFromStartM)
                // Only fire when the segment is long enough to be useful.
                if (rawSegmentLengthM >= MIN_BEAR_SEGMENT_M) {
                    val segmentLengthM = minOf(rawSegmentLengthM, 500.0)
                    events.add(CueEvent.BearRange(upcoming.kind, segmentLengthM))
                }
                announced10m.add(upcoming.id)
            }

            val lastPassed = snapshot.maneuvers
                .filter { snapshot.progressDistanceM - it.distanceFromStartM >= PASSED_TURN_M }
                .maxByOrNull { it.distanceFromStartM }
            if (lastPassed != null && !announcedNextTurnAfter.contains(lastPassed.id)) {
                val indexOfLast = snapshot.maneuvers.indexOfFirst { it.id == lastPassed.id }
                val nextAfter = snapshot.maneuvers.getOrNull(indexOfLast + 1)
                if (nextAfter != null) {
                    // Bug 1 fix: if nextAfter is the last cue maneuver and sits within
                    // CLOSE_TO_DESTINATION_M of the route end, emit ArrivingInM and
                    // suppress all approach cues for it so the phantom "turn X" never plays.
                    val distanceToNext = nextAfter.distanceFromStartM - snapshot.progressDistanceM
                    val isLastManeuver = indexOfLast + 1 == snapshot.maneuvers.size - 1
                    val distNextToEnd = snapshot.routeTotalDistanceM - nextAfter.distanceFromStartM
                    if (isLastManeuver && distNextToEnd < CLOSE_TO_DESTINATION_M) {
                        // Bug 4: when the rider has already crossed the
                        // arrival radius, the dedicated `Arrived` cue at
                        // the bottom of this function speaks instead —
                        // emitting `ArrivingInM` here too produces a
                        // same-tick double cue with disagreeing distances.
                        if (!s.approachingDestinationAnnounced && !snapshot.arrived) {
                            events.add(CueEvent.ArrivingInM(
                                distanceM = snapshot.routeTotalDistanceM - snapshot.progressDistanceM,
                            ))
                            s = s.copy(approachingDestinationAnnounced = true)
                            announcedNextTurnAfter.add(lastPassed.id)
                            announced50m.add(nextAfter.id)
                            announced10m.add(nextAfter.id)
                        }
                    } else if (!nextAfter.isMinorKeep && !isBearKind(nextAfter.kind)) {
                        events.add(
                            CueEvent.NextTurnInAbout(
                                turnKind = nextAfter.kind,
                                distanceM = distanceToNext,
                            ),
                        )
                        announcedNextTurnAfter.add(lastPassed.id)
                        if (distanceToNext < SKIP_50M_BELOW_DISTANCE_M) {
                            announced50m.add(nextAfter.id)
                        }
                    }
                } else if (!s.approachingDestinationAnnounced && !snapshot.arrived) {
                    // Bug 4: skip ArrivingInM when the rider has already
                    // crossed the arrival radius — the `Arrived` cue at
                    // the bottom of this function speaks instead.
                    events.add(
                        CueEvent.ArrivingInM(
                            distanceM = snapshot.routeTotalDistanceM - snapshot.progressDistanceM,
                        ),
                    )
                    announcedNextTurnAfter.add(lastPassed.id)
                    s = s.copy(approachingDestinationAnnounced = true)
                }
            }

            s = s.copy(
                announced50m = announced50m,
                announced10m = announced10m,
                announcedNextTurnAfter = announcedNextTurnAfter,
            )
        }

        if (snapshot.arrived && !s.arrivedAnnounced) {
            events.add(CueEvent.Arrived)
            s = s.copy(arrivedAnnounced = true)
        }

        return Result(
            events = events,
            nextState = s.copy(
                prevOffRoute = snapshot.offRoute,
                prevRerouting = snapshot.rerouting,
                offRouteEpisodeCount = offRouteEpisodeCount,
                offRouteTickCount = offRouteTickCount,
                silenced = silenced,
                consecutiveOnRouteSamples = consecutiveOnRouteSamples,
                onTrackAnnounced = onTrackAnnounced,
                reroutingEpisodeCount = reroutingEpisodeCount,
            ),
        )
    }

    /** Locale-agnostic structured cue: a catalog key + ICU placeholder
     *  values. The wiring layer feeds this to `Strings.t(key, args)`
     *  against the active locale; parity tests render via `Strings.tIn`. */
    data class CueMessage(val key: String, val values: Map<String, Any>)

    /** Map a `CueEvent` to its (key, values) tuple. `distanceMode` chooses
     *  metric vs imperial for spoken distance values. */
    fun cueMessage(
        event: CueEvent,
        distanceMode: app.navon.bike.integration.i18n.DistanceMode =
            app.navon.bike.integration.i18n.DistanceMode.METRIC,
    ): CueMessage = when (event) {
        is CueEvent.Turn50m -> {
            // Use the rider's actual distance to the maneuver, not a
            // hardcoded 50 m — at route start the rider can be 15 m
            // away when the cue first fires, and "in 50 meters turn
            // left" while actually 15 m away is jarringly wrong.
            val baseValues = app.navon.bike.integration.i18n.DistanceFormatter
                .cueValues(event.distanceM, distanceMode)
            val followUp = event.followUpKind
            if (followUp != null) {
                CueMessage(
                    "cue.turn50mCombined",
                    baseValues + mapOf(
                        "first" to maneuverSlug(event.turnKind),
                        "second" to maneuverSlug(followUp),
                    ),
                )
            } else {
                CueMessage(
                    "cue.turn50m.${maneuverSlug(event.turnKind)}",
                    baseValues,
                )
            }
        }
        is CueEvent.Turn10m -> {
            val followUp = event.followUpKind
            if (followUp != null) {
                CueMessage(
                    "cue.turn10mCombined",
                    mapOf(
                        "first" to maneuverSlug(event.turnKind),
                        "second" to maneuverSlug(followUp),
                    ),
                )
            } else {
                CueMessage(
                    "cue.turn10m.${maneuverSlug(event.turnKind)}",
                    emptyMap(),
                )
            }
        }
        is CueEvent.NextTurnInAbout -> CueMessage(
            "cue.nextTurnInAbout.${nextTurnDirection(event.turnKind)}",
            app.navon.bike.integration.i18n.DistanceFormatter
                .cueValues(event.distanceM, distanceMode),
        )
        is CueEvent.BearRange -> CueMessage(
            "cue.bearRange.${maneuverSlug(event.turnKind)}",
            app.navon.bike.integration.i18n.DistanceFormatter
                .cueValues(event.distanceM, distanceMode),
        )
        is CueEvent.ArrivingInM -> CueMessage(
            "cue.arrivingInM",
            app.navon.bike.integration.i18n.DistanceFormatter
                .cueValues(event.distanceM, distanceMode),
        )
        is CueEvent.Arrived -> CueMessage("cue.arrived", emptyMap())
        is CueEvent.OffTrack, is CueEvent.RepeatedOffTrackSilence ->
            CueMessage("cue.offTrack", emptyMap())
        is CueEvent.Rerouting -> CueMessage("cue.rerouting", emptyMap())
        is CueEvent.OnTrack -> CueMessage("cue.onTrack", emptyMap())
    }

    /** Legacy English formatter — kept as the exact-byte path that
     *  existing tests assert against. New call sites should go through
     *  `cueMessage(event)` + `Strings.t(...)` instead. */
    fun format(event: CueEvent): String {
        val msg = cueMessage(event, app.navon.bike.integration.i18n.DistanceMode.METRIC)
        return app.navon.bike.integration.i18n.Strings.tIn(
            app.navon.bike.integration.i18n.SupportedLocale.EN,
            msg.key,
            msg.values,
        )
    }

    private fun maneuverSlug(kind: ManeuverKind): String = when (kind) {
        ManeuverKind.LEFT -> "left"
        ManeuverKind.RIGHT -> "right"
        ManeuverKind.BEAR_LEFT -> "bearLeft"
        ManeuverKind.BEAR_RIGHT -> "bearRight"
        ManeuverKind.EXIT_LEFT -> "exitLeft"
        ManeuverKind.EXIT_RIGHT -> "exitRight"
        ManeuverKind.UTURN -> "uturn"
        ManeuverKind.ROUNDABOUT -> "roundabout"
        ManeuverKind.MERGE -> "merge"
        ManeuverKind.RAMP -> "ramp"
        ManeuverKind.GENERIC -> "generic"
    }

    /** Collapse maneuver kinds into the slugs the `cue.nextTurnInAbout.*`
     *  catalog supports. Exit ramps fold into their parent direction; the
     *  dedicated kinds keep their own slug. */
    private fun nextTurnDirection(kind: ManeuverKind): String = when (kind) {
        ManeuverKind.LEFT, ManeuverKind.EXIT_LEFT -> "left"
        ManeuverKind.RIGHT, ManeuverKind.EXIT_RIGHT -> "right"
        ManeuverKind.BEAR_LEFT -> "bearLeft"
        ManeuverKind.BEAR_RIGHT -> "bearRight"
        ManeuverKind.UTURN -> "uturn"
        ManeuverKind.ROUNDABOUT -> "roundabout"
        ManeuverKind.MERGE -> "merge"
        ManeuverKind.RAMP -> "ramp"
        ManeuverKind.GENERIC -> "generic"
    }
}
