package app.navon.bike.integration.ble.navdevice.beeline

import app.navon.bike.domain.CoordinatePoint
import app.navon.bike.domain.NormalizedRoutePackage
import app.navon.bike.domain.RouteManeuver
import app.navon.bike.integration.ble.navdevice.JunctionIndicator
import app.navon.bike.integration.ble.navdevice.MovingState
import app.navon.bike.integration.ble.navdevice.NavDevice
import java.util.Calendar
import kotlin.math.atan2
import kotlin.math.cos
import kotlin.math.sqrt

/**
 * Translates navon's route model ([NormalizedRoutePackage]) plus live rider
 * fixes into [NavDevice] navigation commands for a Beeline-style device.
 *
 * This is the bridge between navon's provider-agnostic routing world and the
 * Beeline turn-by-turn protocol:
 *
 *   - [start] arms the device for map navigation (startRide → activateMap →
 *     initial polyline + nav frame).
 *   - [onLocation] is called on every GPS tick; it projects the rider onto the
 *     route, derives distance-to-turn / distance-to-destination / progress /
 *     ETA, maps the next maneuver to a Beeline junction indicator, and pushes a
 *     full [NavDevice.sendNavigationUpdate] frame. The route polyline is
 *     refreshed every [POLYLINE_REFRESH_EVERY_N] ticks (the device re-windows
 *     it around the rider internally).
 *   - [stop] returns the device to its idle screen.
 *
 * Heading is derived from successive fixes (navon's [app.navon.bike.domain.LocationState]
 * carries no course), falling back to the bearing toward the next route point
 * when the rider is stationary.
 *
 * Pure JVM logic (no Android imports) so it is unit-testable against a fake
 * [NavDevice]; the only platform touch points live in [BeelineDevice].
 */
class BeelineRouteController(
    private val device: NavDevice,
    /** Fallback planning speed (km/h) used for ETA when the live fix has no usable speed. */
    private val planningSpeedKph: Double = 18.0,
) {
    private var route: NormalizedRoutePackage? = null
    private var cumulative: List<Double> = emptyList()
    private var totalMeters: Double = 0.0

    private var lastRider: CoordinatePoint? = null
    private var lastHeadingDeg: Float = 0f
    private var locationTick: Int = 0
    private var arrived: Boolean = false

    val activeRouteIdentifier: String?
        get() = route?.routeIdentifier

    /**
     * Arm the device for map navigation along [newRoute]. Safe to call again
     * to switch routes (e.g. after a reroute) — resets progress state.
     */
    fun start(newRoute: NormalizedRoutePackage, rider: CoordinatePoint, speedMps: Double?) {
        route = newRoute
        cumulative = cumulativeDistances(newRoute.geometry)
        totalMeters = cumulative.lastOrNull() ?: 0.0
        lastRider = null
        lastHeadingDeg = 0f
        locationTick = 0
        arrived = false

        device.updateGeoMagnetics(rider.latitude, rider.longitude, 0f)
        device.setMovingState(MovingState.ACTIVE)
        device.startRide()
        device.activateMapNavigation()
        sendPolyline(rider)
        onLocation(rider, speedMps)
    }

    /** Push a navigation frame for the current rider fix. No-op if no route is active. */
    fun onLocation(rider: CoordinatePoint, speedMps: Double?) {
        val activeRoute = route ?: return
        val geometry = activeRoute.geometry
        if (geometry.size < 2 || totalMeters <= 0.0) return

        val distanceAlong = projectDistanceAlong(rider, geometry).coerceIn(0.0, totalMeters)
        val distanceToDestination = (totalMeters - distanceAlong).coerceAtLeast(0.0)
        val routeProgress = (distanceAlong / totalMeters).coerceIn(0.0, 1.0)

        val heading = deriveHeading(rider, geometry, distanceAlong)
        lastRider = rider
        lastHeadingDeg = heading

        val speedKmh = ((speedMps ?: 0.0) * 3.6)
        val effectiveSpeedMps = maxOf(speedMps ?: 0.0, planningSpeedKph / 3.6)
        val secondsRemaining = if (effectiveSpeedMps > 0.1) distanceToDestination / effectiveSpeedMps else 0.0
        val minutesRemaining = (secondsRemaining / 60.0).toInt()

        val (etaHour, etaMinute) = etaFromNow(minutesRemaining)

        val nextManeuver = nextManeuverAhead(activeRoute.maneuvers, distanceAlong)
        val distanceToTurn = nextManeuver
            ?.let { (it.distanceFromStartMeters - distanceAlong).coerceAtLeast(0.0) }
            ?: distanceToDestination
        val junction: JunctionIndicator = nextManeuver
            ?.let { BeelineManeuverMapping.junctionFor(it.maneuverType) }
            ?: JunctionIndicator.NONE
        val remainingWaypoints = activeRoute.maneuvers.count { it.distanceFromStartMeters > distanceAlong + 1.0 }
        val roadName = nextManeuver?.instructionText

        if (distanceToDestination <= ARRIVAL_THRESHOLD_M && !arrived) {
            arrived = true
            device.setMovingState(MovingState.ARRIVED)
        } else if (distanceToDestination > ARRIVAL_THRESHOLD_M && arrived) {
            arrived = false
            device.setMovingState(MovingState.ACTIVE)
        }

        device.sendNavigationUpdate(
            distanceToDestination = distanceToDestination.toInt(),
            remainingWaypoints = remainingWaypoints,
            etaHour = etaHour,
            etaMinute = etaMinute,
            timeRemainingHours = minutesRemaining / 60,
            timeRemainingMinutes = minutesRemaining % 60,
            routeProgress = routeProgress,
            elevationProgress = 0.0,
            averageSpeedKmh = speedKmh.toFloat(),
            distanceToTurn = distanceToTurn.toInt(),
            junctionIndicator = junction,
            exitNumber = 0,
            roadName = roadName,
            headingDegrees = heading,
            speedKmh = speedKmh.toFloat(),
            gpsAccuracy = 0,
            elevationMeters = 0,
        )

        // Telemetry: distance covered along the route so far, current speed as
        // the moving average proxy. Sent every tick (SHORT frame).
        device.sendRideTelemetryShort(
            distanceMeters = distanceAlong.toInt(),
            elevationGainMeters = 0,
            avgSpeedCmps = (effectiveSpeedMps * 100).toInt(),
        )

        locationTick += 1
        if (locationTick % POLYLINE_REFRESH_EVERY_N == 0) {
            sendPolyline(rider)
        }
    }

    /** Return the device to its idle screen and forget the active route. */
    fun stop() {
        route?.let {
            device.setMovingState(MovingState.ARRIVED)
            device.endRide()
        }
        route = null
        cumulative = emptyList()
        totalMeters = 0.0
        lastRider = null
        arrived = false
    }

    private fun sendPolyline(rider: CoordinatePoint) {
        val geometry = route?.geometry ?: return
        if (geometry.size < 2) return
        device.sendPolyline(
            points = geometry.map { it.latitude to it.longitude },
            currentLat = rider.latitude,
            currentLng = rider.longitude,
        )
    }

    // ── geometry ────────────────────────────────────────────────────────

    private fun nextManeuverAhead(maneuvers: List<RouteManeuver>, distanceAlong: Double): RouteManeuver? =
        maneuvers.firstOrNull { it.distanceFromStartMeters > distanceAlong + 1.0 }

    /** Bearing of travel: from the last fix when we've moved, else toward the route ahead. */
    private fun deriveHeading(
        rider: CoordinatePoint,
        geometry: List<CoordinatePoint>,
        distanceAlong: Double,
    ): Float {
        lastRider?.let { prev ->
            if (haversine(prev, rider) >= MIN_MOVE_FOR_HEADING_M) {
                return normalizeDegrees(bearingDegrees(prev, rider)).toFloat()
            }
        }
        // Stationary (or first fix): aim at the route point ~20m ahead.
        val target = coordAtDistance(geometry, distanceAlong + 20.0)
        return normalizeDegrees(bearingDegrees(rider, target)).toFloat()
    }

    /** Project [rider] onto the polyline, returning distance-along the route in meters. */
    private fun projectDistanceAlong(rider: CoordinatePoint, geometry: List<CoordinatePoint>): Double {
        var bestDistSq = Double.MAX_VALUE
        var bestAlong = 0.0
        val cosLat = cos(rider.latitude * Math.PI / 180.0)
        fun toXy(p: CoordinatePoint): Pair<Double, Double> =
            ((p.longitude - rider.longitude) * METERS_PER_DEGREE_LAT * cosLat) to
                ((p.latitude - rider.latitude) * METERS_PER_DEGREE_LAT)

        for (i in 0 until geometry.size - 1) {
            val (ax, ay) = toXy(geometry[i])
            val (bx, by) = toXy(geometry[i + 1])
            val dx = bx - ax
            val dy = by - ay
            val segLenSq = dx * dx + dy * dy
            val t = if (segLenSq > 1e-9) (((0 - ax) * dx + (0 - ay) * dy) / segLenSq).coerceIn(0.0, 1.0) else 0.0
            val px = ax + t * dx
            val py = ay + t * dy
            val distSq = px * px + py * py
            if (distSq < bestDistSq) {
                bestDistSq = distSq
                val segLen = cumulative[i + 1] - cumulative[i]
                bestAlong = cumulative[i] + t * segLen
            }
        }
        return bestAlong
    }

    private fun coordAtDistance(geometry: List<CoordinatePoint>, dist: Double): CoordinatePoint {
        if (dist <= 0.0) return geometry.first()
        if (dist >= totalMeters) return geometry.last()
        for (i in 1 until cumulative.size) {
            if (cumulative[i] >= dist) {
                val segLen = cumulative[i] - cumulative[i - 1]
                val t = if (segLen > 1e-9) (dist - cumulative[i - 1]) / segLen else 0.0
                return CoordinatePoint(
                    latitude = geometry[i - 1].latitude + (geometry[i].latitude - geometry[i - 1].latitude) * t,
                    longitude = geometry[i - 1].longitude + (geometry[i].longitude - geometry[i - 1].longitude) * t,
                )
            }
        }
        return geometry.last()
    }

    private fun cumulativeDistances(geometry: List<CoordinatePoint>): List<Double> {
        if (geometry.size < 2) return if (geometry.isEmpty()) emptyList() else listOf(0.0)
        val cum = mutableListOf(0.0)
        for (i in 1 until geometry.size) {
            cum.add(cum[i - 1] + haversine(geometry[i - 1], geometry[i]))
        }
        return cum
    }

    private fun haversine(a: CoordinatePoint, b: CoordinatePoint): Double {
        val dlat = (b.latitude - a.latitude) * METERS_PER_DEGREE_LAT
        val meanLat = ((a.latitude + b.latitude) / 2.0) * (Math.PI / 180.0)
        val dlon = (b.longitude - a.longitude) * cos(meanLat) * METERS_PER_DEGREE_LAT
        return sqrt(dlat * dlat + dlon * dlon)
    }

    private fun bearingDegrees(start: CoordinatePoint, end: CoordinatePoint): Double {
        val latM = (end.latitude - start.latitude) * METERS_PER_DEGREE_LAT
        val meanLat = ((start.latitude + end.latitude) / 2.0) * (Math.PI / 180.0)
        val lonM = (end.longitude - start.longitude) * cos(meanLat) * METERS_PER_DEGREE_LAT
        return Math.toDegrees(atan2(lonM, latM))
    }

    private fun normalizeDegrees(deg: Double): Double {
        var d = deg % 360.0
        if (d < 0) d += 360.0
        return d
    }

    /** Wall-clock ETA [hour, minute] for [minutesAhead] from now. */
    private fun etaFromNow(minutesAhead: Int): Pair<Int, Int> {
        val cal = Calendar.getInstance()
        cal.add(Calendar.MINUTE, minutesAhead.coerceIn(0, 24 * 60))
        return cal.get(Calendar.HOUR_OF_DAY) to cal.get(Calendar.MINUTE)
    }

    private companion object {
        const val METERS_PER_DEGREE_LAT = 111_320.0
        const val ARRIVAL_THRESHOLD_M = 15.0
        const val MIN_MOVE_FOR_HEADING_M = 3.0
        const val POLYLINE_REFRESH_EVERY_N = 5
    }
}
