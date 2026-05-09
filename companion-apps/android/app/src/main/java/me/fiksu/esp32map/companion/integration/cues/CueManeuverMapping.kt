package me.fiksu.esp32map.companion.integration.cues

import me.fiksu.esp32map.companion.domain.RouteManeuverType

/**
 * Single source of truth for "given a [RouteManeuverType], what audio cue
 * kind do we emit?" Returns `null` for maneuver types that produce no cue
 * at all — silence-by-design avoids on-route noise that doesn't match a
 * UI element the rider can act on.
 *
 * Silenced kinds:
 *  - [RouteManeuverType.DEPART] / [RouteManeuverType.ARRIVE] — handled by
 *    dedicated cues ([CueEvent.Arrived], [CueEvent.ArrivingInM]), not the
 *    maneuver stream.
 *  - [RouteManeuverType.SLIGHT_LEFT] / [RouteManeuverType.SLIGHT_RIGHT] —
 *    minor splits the rider follows naturally; "Bear left/right" on every
 *    gentle curve is noise.
 *  - [RouteManeuverType.STRAIGHT] — not a turn; firing
 *    "Next turn in about X meters" / "Follow the route" with no matching
 *    UI element is the bug this filter exists to prevent.
 */
object CueManeuverMapping {
    fun kindFor(type: RouteManeuverType): ManeuverKind? = when (type) {
        RouteManeuverType.LEFT, RouteManeuverType.SHARP_LEFT -> ManeuverKind.LEFT
        RouteManeuverType.RIGHT, RouteManeuverType.SHARP_RIGHT -> ManeuverKind.RIGHT
        RouteManeuverType.UTURN -> ManeuverKind.UTURN
        RouteManeuverType.ROUNDABOUT -> ManeuverKind.ROUNDABOUT
        RouteManeuverType.MERGE -> ManeuverKind.MERGE
        RouteManeuverType.RAMP -> ManeuverKind.RAMP
        RouteManeuverType.SLIGHT_LEFT,
        RouteManeuverType.SLIGHT_RIGHT,
        RouteManeuverType.STRAIGHT,
        RouteManeuverType.DEPART,
        RouteManeuverType.ARRIVE,
        -> null
    }
}
