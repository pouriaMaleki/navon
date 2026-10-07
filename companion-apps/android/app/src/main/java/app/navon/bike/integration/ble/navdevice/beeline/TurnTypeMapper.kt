package app.navon.bike.integration.ble.navdevice.beeline

/**
 * Maps OpenRouteService-style turn type integers to Beeline junction
 * indicator codes (the `y8.b` enum confirmed via Frida captures).
 *
 * Extracted verbatim from the freeline `foss.freeline.nav.api.TurnTypeMapper`.
 * navon's own routing model uses [app.navon.bike.domain.RouteManeuverType]
 * rather than ORS integers — see [BeelineManeuverMapping] for that path —
 * but [NavDevice.mapTurnType] takes an ORS int, so this mapper backs it.
 */
object TurnTypeMapper {
    // ORS step types → Beeline junction indicator values (y8.b enum)
    // Confirmed via Frida captures: 0x100B=TURN_LEFT, 0x100C=TURN_RIGHT
    //
    // ORS types (verified against actual API responses):
    //   0=left, 1=right, 2=sharp left, 3=sharp right,
    //   4=slight left, 5=slight right, 6=straight,
    //   7=enter roundabout, 8=exit roundabout, 9=u-turn,
    //   10=goal/arrive, 11=depart, 12=keep left, 13=keep right
    //
    // Roundabout indicator:
    //   Official app sends 0x0100 (ROUNDABOUT_CW) for left-hand traffic (UK)
    //   or 0x0101 (ROUNDABOUT_CCW) for right-hand traffic (continental Europe).
    //   Exit number is sent separately as the 3rd byte of the 0x1C command.

    /** Whether to use clockwise roundabouts (true = UK/left-hand traffic) */
    var clockwiseRoundabouts = true

    fun orsToBeeline(orsType: Int): Int {
        return when (orsType) {
            0 -> 4107   // TURN_LEFT (0x100B)
            1 -> 4108   // TURN_RIGHT (0x100C)
            2 -> 4109   // TURN_SHARP_LEFT (0x100D)
            3 -> 4110   // TURN_SHARP_RIGHT (0x100E)
            4 -> 4111   // TURN_SLIGHT_LEFT (0x100F)
            5 -> 4112   // TURN_SLIGHT_RIGHT (0x1010)
            6 -> 4113   // STRAIGHT (0x1011)
            7 -> if (clockwiseRoundabouts) 256 else 257  // ROUNDABOUT_CW (0x0100) / CCW (0x0101)
            8 -> 4113   // STRAIGHT (0x1011) — exit roundabout
            9 -> 4136   // U_TURN_LEFT (0x1028)
            10 -> 4096  // ARRIVE_STRAIGHT (0x1000)
            11 -> 4099  // DEPART_STRAIGHT (0x1003)
            12 -> 768   // KEEP_LEFT (0x0300)
            13 -> 769   // KEEP_RIGHT (0x0301)
            else -> 4113 // Default to STRAIGHT
        }
    }

    fun getTurnDescription(beelineCode: Int): String {
        return when (beelineCode) {
            0 -> "No junction"
            4096 -> "Arrive"
            4099 -> "Depart"
            4102 -> "End of road (left)"
            4103 -> "End of road (right)"
            4107 -> "Turn left"
            4108 -> "Turn right"
            4109 -> "Sharp left"
            4110 -> "Sharp right"
            4111 -> "Slight left"
            4112 -> "Slight right"
            4113 -> "Continue straight"
            4114 -> "Merge left"
            4115 -> "Merge right"
            4136 -> "U-turn left"
            4137 -> "U-turn right"
            256 -> "Roundabout (CW)"
            257 -> "Roundabout (CCW)"
            else -> "Continue"
        }
    }
}
