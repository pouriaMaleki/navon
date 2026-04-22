package me.fiksu.esp32map.companion.domain.geo

import me.fiksu.esp32map.companion.domain.CoordinatePoint

/**
 * Approximate bounding box for the Uusimaa region of Finland — Helsinki, Espoo,
 * Vantaa, Porvoo, Hanko, Loviisa, etc. HSL Digitransit only meaningfully covers
 * this area, so any route that starts or ends outside it falls back to OSM.
 *
 * Kept as a pure domain helper so the same bbox can be unit-tested without
 * instantiating `CompanionAppState` (which requires an Android context).
 */
object UusimaaBounds {
    val LATITUDE: ClosedFloatingPointRange<Double> = 59.8..60.8
    val LONGITUDE: ClosedFloatingPointRange<Double> = 23.3..26.7

    fun contains(point: CoordinatePoint): Boolean =
        point.latitude in LATITUDE && point.longitude in LONGITUDE
}
