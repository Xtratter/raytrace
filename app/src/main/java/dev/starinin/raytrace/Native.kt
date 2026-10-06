package dev.starinin.raytrace

import android.view.Surface

object Native {
    init { System.loadLibrary("raytrace") }
    @JvmStatic external fun start(surface: Surface, w: Int, h: Int)
    @JvmStatic external fun stop()
    @JvmStatic external fun resize(w: Int, h: Int)
    @JvmStatic external fun setParam(id: Int, v: Float)
    @JvmStatic external fun orbit(dx: Float, dy: Float)
    @JvmStatic external fun zoom(f: Float)
    @JvmStatic external fun sticks(lx: Float, ly: Float, rx: Float, ry: Float)
    @JvmStatic external fun resetCamera()
    @JvmStatic external fun stats(): FloatArray
}
