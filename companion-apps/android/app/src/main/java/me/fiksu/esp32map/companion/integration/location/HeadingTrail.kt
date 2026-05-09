package me.fiksu.esp32map.companion.integration.location

import me.fiksu.esp32map.companion.domain.CoordinatePoint
import kotlin.math.PI
import kotlin.math.abs
import kotlin.math.atan2
import kotlin.math.cos
import kotlin.math.sqrt

/**
 * Small ring buffer of recent GPS fixes that derives a smoothed travel
 * heading. Matches the web + iOS + runtime-core contract for spec line 110
 * (authoritative): when the rider is moving, the camera rotates to the
 * GPS-derived direction of travel, overriding the route-segment bearing.
 * Returns `null` while stationary / no usable trail.
 */
class HeadingTrail(
    private val maxAgeMs: Long,
    private val maxFixes: Int,
    private val minDisplacementM: Double,
    private val smoothingAlpha: Double,
) {
    private data class Fix(val point: CoordinatePoint, val timestampMs: Long)

    private val fixes: ArrayDeque<Fix> = ArrayDeque()
    private var smoothed: Double? = null

    fun recordFix(point: CoordinatePoint, timestampMs: Long) {
        evictOld(timestampMs)
        fixes.addLast(Fix(point, timestampMs))
        while (fixes.size > maxFixes) fixes.removeFirst()
        val raw = computeRawHeading() ?: return
        val prev = smoothed
        smoothed = if (prev == null) {
            raw
        } else {
            val delta = shortestSignedDelta(prev, raw)
            normalize360(prev + delta * smoothingAlpha)
        }
    }

    val travelHeadingDegrees: Double?
        get() = smoothed

    fun reset() {
        fixes.clear()
        smoothed = null
    }

    private fun evictOld(nowMs: Long) {
        val cutoff = nowMs - maxAgeMs
        while (fixes.isNotEmpty() && fixes.first().timestampMs < cutoff) fixes.removeFirst()
        if (fixes.isEmpty()) smoothed = null
    }

    private fun computeRawHeading(): Double? {
        if (fixes.size < 2) return null
        val first = fixes.first().point
        val last = fixes.last().point
        val metersPerDegLat = 111_320.0
        val meanLat = (first.latitude + last.latitude) / 2.0 * PI / 180.0
        val dNorth = (last.latitude - first.latitude) * metersPerDegLat
        val dEast = (last.longitude - first.longitude) * cos(meanLat) * metersPerDegLat
        val displacement = sqrt(dNorth * dNorth + dEast * dEast)
        if (displacement < minDisplacementM) return null
        return normalize360(atan2(dEast, dNorth) * 180.0 / PI)
    }

    private fun normalize360(deg: Double): Double {
        val r = deg % 360.0
        return if (r < 0) r + 360.0 else r
    }

    private fun shortestSignedDelta(from: Double, to: Double): Double {
        var d = (to - from + 540.0) % 360.0 - 180.0
        if (d == -180.0) d = 180.0
        return d
    }
}

@Suppress("unused")
private fun Double.absLocal() = abs(this)
