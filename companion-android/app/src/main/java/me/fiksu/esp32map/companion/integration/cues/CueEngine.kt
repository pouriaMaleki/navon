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
    object RouteStarted : CueEvent()
    data class Turn50m(val turnKind: ManeuverKind) : CueEvent()
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

    data class Result(val events: List<CueEvent>, val nextState: CueEngineState)

    fun tick(snapshot: CueSnapshot, state: CueEngineState): Result {
        if (snapshot.pairedWithDevice) return Result(emptyList(), state)

        var s: CueEngineState = if (snapshot.routeId != state.lastRouteId) {
            CueEngineState(lastRouteId = snapshot.routeId)
        } else state

        val events = mutableListOf<CueEvent>()

        if (snapshot.routeId != null && !s.routeStartedAnnounced) {
            events.add(CueEvent.RouteStarted)
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
                events.add(CueEvent.Turn50m(upcoming.kind))
                announced50m.add(upcoming.id)
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

    fun format(event: CueEvent): String = when (event) {
        is CueEvent.RouteStarted -> "Route started"
        is CueEvent.Turn50m -> "In 50 meters, ${turnVerb(event.turnKind)}"
        is CueEvent.Turn10m -> turnImperative(event.turnKind)
        is CueEvent.NextTurnInAbout ->
            "Next turn ${turnDirectionWord(event.turnKind)} in about ${roundTo10(event.distanceM)} meters"
        is CueEvent.ArrivingInM ->
            "Arriving at your destination in ${roundTo10(event.distanceM)} meters"
        is CueEvent.Arrived -> "You have arrived at your destination"
        is CueEvent.OffTrack, is CueEvent.RepeatedOffTrackSilence -> "Off track"
        is CueEvent.Rerouting -> "Rerouting"
        is CueEvent.OnTrack -> "On track"
    }

    private fun turnVerb(kind: ManeuverKind): String = when (kind) {
        ManeuverKind.LEFT -> "turn left"
        ManeuverKind.RIGHT -> "turn right"
        ManeuverKind.KEEP_LEFT -> "keep left"
        ManeuverKind.KEEP_RIGHT -> "keep right"
        ManeuverKind.EXIT_LEFT -> "take the left exit"
        ManeuverKind.EXIT_RIGHT -> "take the right exit"
        ManeuverKind.UTURN -> "make a U-turn"
        ManeuverKind.GENERIC -> "follow the route"
    }

    private fun turnImperative(kind: ManeuverKind): String = when (kind) {
        ManeuverKind.LEFT -> "Turn left"
        ManeuverKind.RIGHT -> "Turn right"
        ManeuverKind.KEEP_LEFT -> "Keep left"
        ManeuverKind.KEEP_RIGHT -> "Keep right"
        ManeuverKind.EXIT_LEFT -> "Take the left exit"
        ManeuverKind.EXIT_RIGHT -> "Take the right exit"
        ManeuverKind.UTURN -> "Make a U-turn"
        ManeuverKind.GENERIC -> "Follow the route"
    }

    private fun turnDirectionWord(kind: ManeuverKind): String = when (kind) {
        ManeuverKind.LEFT, ManeuverKind.KEEP_LEFT, ManeuverKind.EXIT_LEFT -> "left"
        ManeuverKind.RIGHT, ManeuverKind.KEEP_RIGHT, ManeuverKind.EXIT_RIGHT -> "right"
        ManeuverKind.UTURN -> "u-turn"
        ManeuverKind.GENERIC -> "ahead"
    }

    private fun roundTo10(meters: Double): Long = Math.round(meters / 10.0) * 10
}
