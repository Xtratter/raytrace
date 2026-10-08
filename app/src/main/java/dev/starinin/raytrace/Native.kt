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
    /** Parses a scene file (JSON) and switches to it; returns "" on success or a readable error. */
    @JvmStatic external fun loadScene(json: String): String
    /** 16 floats: [0..5] fps, gpu ms, w, h, scale, path; [8..15] per-pass GPU ms (trace, gi_trace, gi_temporal, gi_atrous, temporal, atrous, composite, present). */
    @JvmStatic external fun stats(): FloatArray
}
