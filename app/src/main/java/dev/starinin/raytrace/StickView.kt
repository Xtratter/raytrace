package dev.starinin.raytrace

import android.content.Context
import android.graphics.Canvas
import android.graphics.Paint
import android.view.View
import io.github.xtratter.uikit.M3

/** One on-screen stick: translucent ring plus a thumb that follows the deflection. Drawing only, no touch handling. */
class StickView(ctx: Context) : View(ctx) {
    private val dp = ctx.resources.displayMetrics.density
    private val fillP = Paint(Paint.ANTI_ALIAS_FLAG).apply { style = Paint.Style.FILL }
    private val ringP = Paint(Paint.ANTI_ALIAS_FLAG).apply { style = Paint.Style.STROKE; strokeWidth = 2f * dp }
    private val thumbP = Paint(Paint.ANTI_ALIAS_FLAG).apply { style = Paint.Style.FILL }
    private var nx = 0f; private var ny = 0f   // deflection, -1..1, y up

    /** Distance from the centre the thumb centre can reach, in px (for the current size). */
    val travel get() = width / 2f * TRAVEL

    init { restyle() }

    /** Deflection in -1..1 (x right, y up). */
    fun setThumb(x: Float, y: Float) { nx = x; ny = y; invalidate() }

    /** Re-read the theme colours. */
    fun restyle() {
        fillP.color = M3.ink(0x33)
        ringP.color = M3.ink(0x66)
        thumbP.color = M3.primary; thumbP.alpha = 230
        invalidate()
    }

    override fun onMeasure(w: Int, h: Int) {
        val s = (150f * dp).toInt(); setMeasuredDimension(s, s)
    }

    override fun onDraw(c: Canvas) {
        val cx = width / 2f; val cy = height / 2f; val r = width / 2f - ringP.strokeWidth
        c.drawCircle(cx, cy, r, fillP)
        c.drawCircle(cx, cy, r, ringP)
        c.drawCircle(cx + nx * travel, cy - ny * travel, width / 2f * THUMB, thumbP)
    }

    companion object { const val THUMB = 0.32f; const val TRAVEL = 0.62f }
}
