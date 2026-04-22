package me.fiksu.esp32map.companion.flows

import me.fiksu.esp32map.companion.domain.CoordinatePoint
import me.fiksu.esp32map.companion.domain.geo.UusimaaBounds
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * L1 tests — HSL applicability bounding-box semantics (plan flow #31 /
 * hsl_skipped_outside_uusimaa). Exercises the real `UusimaaBounds.contains`
 * used by `CompanionAppState.isInUusimaa`, so a regression in the shipped code
 * surfaces here.
 */
class HslGatingTest {
    @Test
    fun helsinki_inside_bounds() {
        assertTrue(UusimaaBounds.contains(CoordinatePoint(60.1699, 24.9384)))
    }

    @Test
    fun tampere_outside_bounds() {
        assertFalse(UusimaaBounds.contains(CoordinatePoint(61.4978, 23.7610)))
    }

    @Test
    fun stockholm_far_south_rejected() {
        assertFalse(UusimaaBounds.contains(CoordinatePoint(59.3293, 18.0686)))
    }

    @Test
    fun stPetersburg_far_east_rejected() {
        assertFalse(UusimaaBounds.contains(CoordinatePoint(59.9311, 30.3609)))
    }

    @Test
    fun porvoo_east_edge_still_inside() {
        assertTrue(UusimaaBounds.contains(CoordinatePoint(60.3907, 25.6615)))
    }

    @Test
    fun just_north_of_bbox_rejected() {
        assertFalse(UusimaaBounds.contains(CoordinatePoint(60.9, 24.9)))
    }
}
