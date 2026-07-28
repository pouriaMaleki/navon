package app.navon.bike.integration.ble.navdevice.beeline

import app.navon.bike.domain.RouteManeuverType
import app.navon.bike.integration.ble.navdevice.JunctionIndicator

/**
 * Maps navon's own [RouteManeuverType] onto Beeline junction-indicator codes.
 *
 * The freeline port ships [TurnTypeMapper] (ORS integers → Beeline), which
 * backs the generic [app.navon.bike.integration.ble.navdevice.NavDevice.mapTurnType]
 * escape hatch. navon's routing layer, however, emits a typed
 * [RouteManeuverType] enum rather than ORS integers, so the route controller
 * maps through here instead — keeping the device-facing turn semantics in one
 * place and independent of any upstream provider's integer codes.
 */
object BeelineManeuverMapping {

    /**
     * Whether roundabouts render clockwise (UK / left-hand traffic). Kept in
     * sync with [TurnTypeMapper.clockwiseRoundabouts] so both turn-mapping
     * paths agree on roundabout direction.
     */
    var clockwiseRoundabouts: Boolean
        get() = TurnTypeMapper.clockwiseRoundabouts
        set(value) {
            TurnTypeMapper.clockwiseRoundabouts = value
        }

    fun junctionFor(type: RouteManeuverType): JunctionIndicator = JunctionIndicator(
        when (type) {
            RouteManeuverType.DEPART -> BeelineProtocol.DEPART_STRAIGHT
            RouteManeuverType.STRAIGHT -> BeelineProtocol.STRAIGHT
            RouteManeuverType.SLIGHT_LEFT -> BeelineProtocol.TURN_SLIGHT_LEFT
            RouteManeuverType.LEFT -> BeelineProtocol.TURN_LEFT
            RouteManeuverType.SHARP_LEFT -> BeelineProtocol.TURN_SHARP_LEFT
            RouteManeuverType.SLIGHT_RIGHT -> BeelineProtocol.TURN_SLIGHT_RIGHT
            RouteManeuverType.RIGHT -> BeelineProtocol.TURN_RIGHT
            RouteManeuverType.SHARP_RIGHT -> BeelineProtocol.TURN_SHARP_RIGHT
            RouteManeuverType.UTURN -> BeelineProtocol.U_TURN_LEFT
            RouteManeuverType.ROUNDABOUT ->
                if (clockwiseRoundabouts) BeelineProtocol.ROUNDABOUT_CLOCKWISE
                else BeelineProtocol.ROUNDABOUT_ANTI_CLOCKWISE
            RouteManeuverType.MERGE -> BeelineProtocol.MERGE_LEFT
            RouteManeuverType.RAMP -> BeelineProtocol.OFF_RAMP_RIGHT
            RouteManeuverType.ARRIVE -> BeelineProtocol.ARRIVE_STRAIGHT
        }.toInt()
    )
}
