package dev.starinin.raytrace

import android.animation.ValueAnimator
import android.content.Context
import android.view.MotionEvent
import android.view.View
import android.view.Gravity
import android.widget.FrameLayout
import io.github.xtratter.uikit.Haptics
import kotlin.math.hypot

/**
 * Transparent overlay with two on-screen sticks (left: movement, right: look). Each stick follows its own pointer, so
 * both work at once. Touches outside the sticks fall through to the view below (the render view).
 */
class SticksLayer(ctx: Context) : FrameLayout(ctx) {
    var onChange: (Float, Float, Float, Float) -> Unit = { _, _, _, _ -> }
    private val dp = ctx.resources.displayMetrics.density
    private val views = arrayOf(StickView(ctx), StickView(ctx))   // 0 left, 1 right
    private val ids = intArrayOf(-1, -1)
    private val defl = Array(2) { floatArrayOf(0f, 0f) }          // raw deflection of each thumb, -1..1, y up
    private val anims = arrayOfNulls<ValueAnimator>(2)

    init {
        val m = (24f * dp).toInt(); val mb = (40f * dp).toInt()
        addView(views[0], LayoutParams(-2, -2, Gravity.BOTTOM or Gravity.START).apply { setMargins(m, 0, 0, mb) })
        addView(views[1], LayoutParams(-2, -2, Gravity.BOTTOM or Gravity.END).apply { setMargins(0, 0, m, mb) })
    }

    private fun hit(i: Int, x: Float, y: Float): Boolean {
        val v = views[i]; if (v.width == 0) return false
        val r = v.width / 2f * 1.3f
        return hypot(x - (v.left + v.width / 2f), y - (v.top + v.height / 2f)) <= r
    }

    private fun report() {
        val l = StickMath.shape(defl[0][0], defl[0][1]); val r = StickMath.shape(defl[1][0], defl[1][1])
        onChange(l.first, l.second, r.first, r.second)
    }

    private fun track(i: Int, e: MotionEvent) {
        val p = e.findPointerIndex(ids[i]); if (p < 0) return
        val v = views[i]; val t = v.travel; if (t <= 0f) return
        var dx = (e.getX(p) - (v.left + v.width / 2f)) / t
        var dy = -(e.getY(p) - (v.top + v.height / 2f)) / t   // screen y grows downward
        val m = hypot(dx, dy); if (m > 1f) { dx /= m; dy /= m }
        defl[i][0] = dx; defl[i][1] = dy
        v.setThumb(dx, dy)
    }

    private fun springBack(i: Int) {
        anims[i]?.cancel()
        val x0 = defl[i][0]; val y0 = defl[i][1]
        defl[i][0] = 0f; defl[i][1] = 0f
        if (x0 == 0f && y0 == 0f) { views[i].setThumb(0f, 0f); return }
        anims[i] = ValueAnimator.ofFloat(1f, 0f).apply {
            duration = 120
            addUpdateListener { val f = it.animatedValue as Float; views[i].setThumb(x0 * f, y0 * f) }
            start()
        }
    }

    private fun releaseStick(i: Int) { ids[i] = -1; springBack(i) }

    override fun dispatchTouchEvent(e: MotionEvent): Boolean {
        if (visibility != VISIBLE) return false
        val a = e.actionMasked
        if (a == MotionEvent.ACTION_DOWN && ids[0] < 0 && ids[1] < 0 && !hit(0, e.x, e.y) && !hit(1, e.x, e.y)) return false
        when (a) {
            MotionEvent.ACTION_DOWN, MotionEvent.ACTION_POINTER_DOWN -> {
                val k = e.actionIndex; val pid = e.getPointerId(k)
                val i = (0..1).firstOrNull { ids[it] < 0 && hit(it, e.getX(k), e.getY(k)) }
                if (i != null) {
                    anims[i]?.cancel(); ids[i] = pid
                    Haptics.play(Haptics.Kind.TICK)
                    track(i, e); report()
                }
            }
            MotionEvent.ACTION_MOVE -> {
                if (ids[0] >= 0) track(0, e)
                if (ids[1] >= 0) track(1, e)
                report()
            }
            MotionEvent.ACTION_POINTER_UP, MotionEvent.ACTION_UP -> {
                val pid = e.getPointerId(e.actionIndex)
                (0..1).forEach { if (ids[it] == pid) releaseStick(it) }
                report()
            }
            MotionEvent.ACTION_CANCEL -> { releaseStick(0); releaseStick(1); report() }
        }
        return true
    }

    /** Hidden sticks release and report zeros once. */
    fun setSticksVisible(v: Boolean) {
        if (v) { visibility = VISIBLE; return }
        if (visibility == GONE) return
        release(); visibility = GONE
    }

    fun restyle() = views.forEach { it.restyle() }

    /** Zero everything (pause, hide) and report it. */
    fun release() {
        for (i in 0..1) { ids[i] = -1; anims[i]?.cancel(); defl[i][0] = 0f; defl[i][1] = 0f; views[i].setThumb(0f, 0f) }
        report()
    }
}
