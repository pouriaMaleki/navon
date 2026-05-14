package app.navon.bike.integration.i18n

import kotlin.math.roundToInt
import kotlin.math.roundToLong

/**
 * Distance formatting helpers — bridges raw meters to the
 * (number, unit) tuple consumed by cue ICU templates and produces
 * ready-to-render UI labels via `units.distance.*` keys.
 */
object DistanceFormatter {
    private const val FT_PER_M = 3.280839895
    private const val MI_PER_M = 0.0006213712
    private const val KM_THRESHOLD_M = 1000.0
    private const val MI_THRESHOLD_M = 1609.0

    fun roundTo10(n: Double): Long = ((n / 10.0).roundToLong()) * 10L

    /** ICU placeholder bundle for a distance voice cue. */
    fun cueValues(meters: Double, mode: DistanceMode): Map<String, Any> = when (mode) {
        DistanceMode.IMPERIAL -> mapOf(
            "distance" to roundTo10(meters * FT_PER_M),
            "distanceUnit" to "feet",
        )
        DistanceMode.METRIC -> {
            if (meters >= KM_THRESHOLD_M) {
                val km = (meters / 100.0).roundToInt() / 10.0 // one decimal
                mapOf(
                    "distance" to km,
                    "distanceUnit" to "kilometers",
                )
            } else {
                mapOf(
                    "distance" to roundTo10(meters),
                    "distanceUnit" to "meters",
                )
            }
        }
    }

    /** UI label for an arbitrary distance in meters. */
    fun label(meters: Double, mode: DistanceMode): String = when (mode) {
        DistanceMode.IMPERIAL -> {
            if (meters >= MI_THRESHOLD_M) {
                val miles = meters * MI_PER_M
                Strings.t(
                    "units.distance.mi",
                    mapOf("distance" to (miles * 10).roundToInt() / 10.0),
                )
            } else {
                Strings.t(
                    "units.distance.ft",
                    mapOf("distance" to (meters * FT_PER_M).roundToInt()),
                )
            }
        }
        DistanceMode.METRIC -> {
            if (meters >= KM_THRESHOLD_M) {
                val km = meters / 1000.0
                Strings.t(
                    "units.distance.km",
                    mapOf("distance" to (km * 10).roundToInt() / 10.0),
                )
            } else {
                Strings.t(
                    "units.distance.m",
                    mapOf("distance" to meters.roundToInt()),
                )
            }
        }
    }
}
