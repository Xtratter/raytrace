package dev.starinin.raytrace

import android.content.Context
import android.view.*

class RenderView(ctx: Context, private val settings: Settings) : SurfaceView(ctx), SurfaceHolder.Callback {
    private var lastX = 0f; private var lastY = 0f; private var fingers = 0
    /** The engine runs on the current surface (started on its first surfaceChanged, stopped on surfaceDestroyed). */
    private var running = false
    private val pinch = ScaleGestureDetector(ctx, object : ScaleGestureDetector.SimpleOnScaleGestureListener() {
        override fun onScale(d: ScaleGestureDetector): Boolean { Native.zoom(1f / d.scaleFactor); return true }
    })
    private val taps = GestureDetector(ctx, object : GestureDetector.SimpleOnGestureListener() {
        override fun onDoubleTap(e: MotionEvent): Boolean { Native.resetCamera(); return true }
    })

    init { holder.addCallback(this) }

    override fun surfaceCreated(h: SurfaceHolder) { running = false }
    override fun surfaceChanged(h: SurfaceHolder, f: Int, w: Int, hh: Int) {
        if (running) { Native.resize(w, hh); return }   // same surface, new size: keep the engine and the camera
        Native.start(h.surface, w, hh)
        settings.pushAll()
        running = true
    }
    override fun surfaceDestroyed(h: SurfaceHolder) { Native.stop(); running = false }

    override fun onTouchEvent(e: MotionEvent): Boolean {
        pinch.onTouchEvent(e); taps.onTouchEvent(e)
        when (e.actionMasked) {
            MotionEvent.ACTION_DOWN -> { lastX = e.x; lastY = e.y; fingers = 1 }
            MotionEvent.ACTION_POINTER_DOWN -> fingers = e.pointerCount
            MotionEvent.ACTION_POINTER_UP -> { fingers = e.pointerCount - 1; val i = if (e.actionIndex == 0) 1 else 0; lastX = e.getX(i); lastY = e.getY(i) }
            MotionEvent.ACTION_MOVE -> if (fingers == 1 && !pinch.isInProgress) {
                Native.orbit(e.x - lastX, e.y - lastY); lastX = e.x; lastY = e.y
            }
        }
        return true
    }
}
