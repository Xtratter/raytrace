package dev.starinin.raytrace

import kotlin.math.hypot

/** Pure joystick shaping (no Android): dead zone plus a quadratic response for fine control near the centre. */
object StickMath {
    /**
     * [x], [y]: thumb offset divided by the stick radius (x right, y up; may exceed unit length).
     * Returns the shaped vector with the same direction and magnitude in 0..1.
     */
    fun shape(x: Float, y: Float, dead: Float = 0.12f): Pair<Float, Float> {
        if (!x.isFinite() || !y.isFinite()) return 0f to 0f
        val raw = hypot(x, y)
        if (!raw.isFinite()) return 0f to 0f
        val m = raw.coerceAtMost(1f)
        if (m < dead || m <= 0f) return 0f to 0f
        val n = ((m - dead) / (1f - dead)).coerceIn(0f, 1f)
        val k = n * n / raw   // direction x/raw scaled by the shaped magnitude
        return x * k to y * k
    }
}
