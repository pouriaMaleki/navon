package app.navon.bike.domain.geo

import app.navon.bike.domain.CoordinatePoint

/**
 * Approximate bounding box for mainland Finland (including Åland). Digitransit's
 * `finland` router aggregates GTFS feeds nationwide, so any route with both
 * endpoints inside this box is eligible for HSL/Digitransit routing; anything
 * outside falls back to OSM.
 *
 * Kept as a pure domain helper so the same bbox can be unit-tested without
 * instantiating `CompanionAppState` (which requires an Android context).
 */
object FinlandBounds {
    val LATITUDE: ClosedFloatingPointRange<Double> = 59.7..70.1
    val LONGITUDE: ClosedFloatingPointRange<Double> = 19.0..31.7

    fun contains(point: CoordinatePoint): Boolean =
        point.latitude in LATITUDE && point.longitude in LONGITUDE
}
