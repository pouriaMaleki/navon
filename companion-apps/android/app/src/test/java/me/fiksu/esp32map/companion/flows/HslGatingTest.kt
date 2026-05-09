package me.fiksu.esp32map.companion.flows

import me.fiksu.esp32map.companion.domain.CoordinatePoint
import me.fiksu.esp32map.companion.domain.geo.FinlandBounds
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * L1 tests — HSL applicability bounding-box semantics (plan flow #31 /
 * hsl_skipped_outside_finland). Exercises the real `FinlandBounds.contains`
 * used by `CompanionAppState.isInFinland`, so a regression in the shipped code
 * surfaces here.
 */
class HslGatingTest {
    @Test
    fun helsinki_inside_bounds() {
        assertTrue(FinlandBounds.contains(CoordinatePoint(60.1699, 24.9384)))
    }

    @Test
    fun tampere_inside_bounds() {
        assertTrue(FinlandBounds.contains(CoordinatePoint(61.4978, 23.7610)))
    }

    @Test
    fun rovaniemi_inside_bounds() {
        assertTrue(FinlandBounds.contains(CoordinatePoint(66.5039, 25.7294)))
    }

    @Test
    fun stockholm_west_rejected() {
        assertFalse(FinlandBounds.contains(CoordinatePoint(59.3293, 18.0686)))
    }

    @Test
    fun tallinn_south_rejected() {
        assertFalse(FinlandBounds.contains(CoordinatePoint(59.4370, 24.7536)))
    }

    @Test
    fun far_north_arctic_rejected() {
        assertFalse(FinlandBounds.contains(CoordinatePoint(71.0, 25.0)))
    }
}
