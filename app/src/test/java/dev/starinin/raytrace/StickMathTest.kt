package dev.starinin.raytrace

import org.junit.Assert.*
import org.junit.Test
import kotlin.math.hypot

class StickMathTest {
    private fun mag(p: Pair<Float, Float>) = hypot(p.first, p.second)

    @Test fun deadZoneGivesZero() {
        assertEquals(0f to 0f, StickMath.shape(0.05f, 0.05f))
        assertEquals(0f to 0f, StickMath.shape(0f, 0f))
        assertEquals(0f to 0f, StickMath.shape(0.119f, 0f))
    }

    @Test fun fullDeflectionIsOne() {
        assertEquals(1f, mag(StickMath.shape(1f, 0f)), 1e-5f)
        assertEquals(1f, mag(StickMath.shape(0f, -1f)), 1e-5f)
    }

    @Test fun clampsAboveUnit() {
        val p = StickMath.shape(3f, 4f)
        assertEquals(1f, mag(p), 1e-5f)
        assertEquals(0.6f, p.first, 1e-5f); assertEquals(0.8f, p.second, 1e-5f)
    }

    @Test fun monotonic() {
        var prev = -1f
        for (i in 0..100) { val m = mag(StickMath.shape(i / 100f, 0f)); assertTrue(m >= prev); prev = m }
    }

    @Test fun quadraticCurve() {
        // halfway between dead zone and the edge -> 0.25
        val m = 0.12f + (1f - 0.12f) / 2f
        assertEquals(0.25f, mag(StickMath.shape(m, 0f)), 1e-5f)
    }

    @Test fun directionPreserved() {
        val p = StickMath.shape(0.3f, 0.4f)
        assertTrue(p.first > 0f && p.second > 0f)
        assertEquals(0.75f, p.first / p.second, 1e-4f)
    }

    @Test fun symmetric() {
        val a = StickMath.shape(0.5f, 0.3f); val b = StickMath.shape(-0.5f, -0.3f)
        assertEquals(-a.first, b.first, 1e-6f); assertEquals(-a.second, b.second, 1e-6f)
    }

    @Test fun nonFiniteIsZero() {
        assertEquals(0f to 0f, StickMath.shape(Float.NaN, 0.5f))
        assertEquals(0f to 0f, StickMath.shape(0.5f, Float.POSITIVE_INFINITY))
    }
}
