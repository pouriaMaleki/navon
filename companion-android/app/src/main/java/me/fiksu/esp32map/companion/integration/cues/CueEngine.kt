package me.fiksu.esp32map.companion.integration.cues

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
    KEEP_LEFT,
    KEEP_RIGHT,
    EXIT_LEFT,
    EXIT_RIGHT,
    UTURN,
    GENERIC;
}

data class CueManeuver(
    val id: String,
    val kind: ManeuverKind,
    val distanceFromStartM: Double,
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
    /** 50m before a turn. When the next maneuver is within 80m,
     *  `followUpKind` carries that maneuver's direction so the engine
     *  emits a single combined cue ("In 50 meters, turn right then
     *  quickly left") instead of overlapping audio. */
    data class Turn50m(
        val turnKind: ManeuverKind,
        val followUpKind: ManeuverKind? = null,
    ) : CueEvent()
    data class Turn10m(val turnKind: ManeuverKind) : CueEvent()
    data class NextTurnInAbout(val turnKind: ManeuverKind, val distanceM: Double) : CueEvent()
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
)

object CueEngine {
    private const val APPROACH_50_M = 50.0
    private const val APPROACH_10_M = 10.0
    private const val PASSED_TURN_M = 10.0
    private const val ON_TRACK_CONFIRM_SAMPLES = 5
    private const val ON_TRACK_CORRIDOR_M = 22.0
    private const val REPEAT_OFFTRACK_SILENCE_THRESHOLD = 2
    /** Two maneuvers separated by less than this fold into a single
     *  "turn X then quickly Y" cue. Mirrors web's BACK_TO_BACK_THRESHOLD_M. */
    private const val BACK_TO_BACK_THRESHOLD_M = 80.0

    data class Result(val events: List<CueEvent>, val nextState: CueEngineState)

    fun tick(snapshot: CueSnapshot, state: CueEngineState): Result {
        if (snapshot.pairedWithDevice) return Result(emptyList(), state)

        var s: CueEngineState = if (snapshot.routeId != state.lastRouteId) {
            CueEngineState(lastRouteId = snapshot.routeId)
        } else state

        val events = mutableListOf<CueEvent>()

        // First-tick announcement (replaces "Route started"). User-feedback:
        // "Route started" was useless padding. Replace with the actual
        // next-turn announcement so the rider hears something immediately
        // useful on Start.
        if (snapshot.routeId != null && !s.routeStartedAnnounced) {
            val firstNonDepart = snapshot.maneuvers.firstOrNull {
                it.distanceFromStartM - snapshot.progressDistanceM >= 0
            }
            if (firstNonDepart != null) {
                events.add(
                    CueEvent.NextTurnInAbout(
                        turnKind = firstNonDepart.kind,
                        distanceM = firstNonDepart.distanceFromStartM - snapshot.progressDistanceM,
                    ),
                )
            }
            s = s.copy(routeStartedAnnounced = true)
        }

        val offRouteRose = !s.prevOffRoute && snapshot.offRoute
        var offRouteEpisodeCount = s.offRouteEpisodeCount
        if (offRouteRose) offRouteEpisodeCount += 1

        var silenced = s.silenced
        var onTrackAnnounced = s.onTrackAnnounced
        var consecutiveOnRouteSamples = s.consecutiveOnRouteSamples

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

        if (offRouteRose && offRouteEpisodeCount > REPEAT_OFFTRACK_SILENCE_THRESHOLD && !silenced) {
            events.add(CueEvent.RepeatedOffTrackSilence)
            silenced = true
            onTrackAnnounced = false
        } else if (offRouteRose && !silenced) {
            events.add(CueEvent.OffTrack)
        }

        val reroutingRose = !s.prevRerouting && snapshot.rerouting
        if (reroutingRose && !silenced) {
            events.add(CueEvent.Rerouting)
        }

        if (!silenced && !snapshot.offRoute) {
            val announced50m = s.announced50m.toMutableSet()
            val announced10m = s.announced10m.toMutableSet()
            val announcedNextTurnAfter = s.announcedNextTurnAfter.toMutableSet()

            val upcoming = snapshot.maneuvers.firstOrNull {
                it.distanceFromStartM - snapshot.progressDistanceM >= 0
            }
            val upcomingDistance = upcoming?.let { it.distanceFromStartM - snapshot.progressDistanceM }

            if (upcoming != null && upcomingDistance != null &&
                upcomingDistance <= APPROACH_50_M && upcomingDistance > APPROACH_10_M &&
                !announced50m.contains(upcoming.id)
            ) {
                val upcomingIdx = snapshot.maneuvers.indexOfFirst { it.id == upcoming.id }
                val followUp = snapshot.maneuvers.getOrNull(upcomingIdx + 1)
                val gap = followUp?.let { it.distanceFromStartM - upcoming.distanceFromStartM }
                    ?: Double.POSITIVE_INFINITY
                if (followUp != null && gap <= BACK_TO_BACK_THRESHOLD_M) {
                    events.add(CueEvent.Turn50m(upcoming.kind, followUpKind = followUp.kind))
                    announced50m.add(upcoming.id)
                    announced50m.add(followUp.id)
                    announced10m.add(followUp.id)
                    announcedNextTurnAfter.add(upcoming.id)
                } else {
                    events.add(CueEvent.Turn50m(upcoming.kind, followUpKind = null))
                    announced50m.add(upcoming.id)
                }
            }
            if (upcoming != null && upcomingDistance != null &&
                upcomingDistance <= APPROACH_10_M && !announced10m.contains(upcoming.id)
            ) {
                events.add(CueEvent.Turn10m(upcoming.kind))
                announced10m.add(upcoming.id)
            }

            val lastPassed = snapshot.maneuvers
                .filter { snapshot.progressDistanceM - it.distanceFromStartM >= PASSED_TURN_M }
                .maxByOrNull { it.distanceFromStartM }
            if (lastPassed != null && !announcedNextTurnAfter.contains(lastPassed.id)) {
                val indexOfLast = snapshot.maneuvers.indexOfFirst { it.id == lastPassed.id }
                val nextAfter = snapshot.maneuvers.getOrNull(indexOfLast + 1)
                if (nextAfter != null) {
                    events.add(
                        CueEvent.NextTurnInAbout(
                            turnKind = nextAfter.kind,
                            distanceM = nextAfter.distanceFromStartM - snapshot.progressDistanceM,
                        ),
                    )
                    announcedNextTurnAfter.add(lastPassed.id)
                } else if (!s.approachingDestinationAnnounced) {
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
                silenced = silenced,
                consecutiveOnRouteSamples = consecutiveOnRouteSamples,
                onTrackAnnounced = onTrackAnnounced,
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
        distanceMode: me.fiksu.esp32map.companion.integration.i18n.DistanceMode =
            me.fiksu.esp32map.companion.integration.i18n.DistanceMode.METRIC,
    ): CueMessage = when (event) {
        is CueEvent.Turn50m -> {
            val baseValues = me.fiksu.esp32map.companion.integration.i18n.DistanceFormatter
                .cueValues(50.0, distanceMode)
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
        is CueEvent.Turn10m -> CueMessage(
            "cue.turn10m.${maneuverSlug(event.turnKind)}",
            emptyMap(),
        )
        is CueEvent.NextTurnInAbout -> CueMessage(
            "cue.nextTurnInAbout.${nextTurnDirection(event.turnKind)}",
            me.fiksu.esp32map.companion.integration.i18n.DistanceFormatter
                .cueValues(event.distanceM, distanceMode),
        )
        is CueEvent.ArrivingInM -> CueMessage(
            "cue.arrivingInM",
            me.fiksu.esp32map.companion.integration.i18n.DistanceFormatter
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
        val msg = cueMessage(event, me.fiksu.esp32map.companion.integration.i18n.DistanceMode.METRIC)
        return me.fiksu.esp32map.companion.integration.i18n.Strings.tIn(
            me.fiksu.esp32map.companion.integration.i18n.SupportedLocale.EN,
            msg.key,
            msg.values,
        )
    }

    private fun maneuverSlug(kind: ManeuverKind): String = when (kind) {
        ManeuverKind.LEFT -> "left"
        ManeuverKind.RIGHT -> "right"
        ManeuverKind.KEEP_LEFT -> "keepLeft"
        ManeuverKind.KEEP_RIGHT -> "keepRight"
        ManeuverKind.EXIT_LEFT -> "exitLeft"
        ManeuverKind.EXIT_RIGHT -> "exitRight"
        ManeuverKind.UTURN -> "uturn"
        ManeuverKind.GENERIC -> "generic"
    }

    /** Collapse 8 maneuver kinds into the 4 directions the
     *  `cue.nextTurnInAbout.*` catalog supports. */
    private fun nextTurnDirection(kind: ManeuverKind): String = when (kind) {
        ManeuverKind.LEFT, ManeuverKind.KEEP_LEFT, ManeuverKind.EXIT_LEFT -> "left"
        ManeuverKind.RIGHT, ManeuverKind.KEEP_RIGHT, ManeuverKind.EXIT_RIGHT -> "right"
        ManeuverKind.UTURN -> "uturn"
        ManeuverKind.GENERIC -> "generic"
    }
}
