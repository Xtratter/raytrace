package dev.starinin.raytrace

import android.app.Activity
import android.os.Build
import android.os.Bundle
import android.view.*

class MainActivity : Activity() {
    lateinit var settings: Settings
    override fun onCreate(b: Bundle?) {
        super.onCreate(b)
        val sp = getSharedPreferences("prefs", MODE_PRIVATE)
        settings = Settings(object : KeyValue {
            override fun getFloat(k: String, d: Float) = sp.getFloat(k, d)
            override fun putFloat(k: String, v: Float) { sp.edit().putFloat(k, v).apply() }
        }) { id, v -> Native.setParam(id, v) }
        window.addFlags(WindowManager.LayoutParams.FLAG_KEEP_SCREEN_ON)
        setContentView(RenderView(this, settings))
    }
    override fun onWindowFocusChanged(f: Boolean) {
        super.onWindowFocusChanged(f)
        if (f) hideSystemBars()
    }
    private fun hideSystemBars() {
        if (Build.VERSION.SDK_INT >= 30) {
            window.setDecorFitsSystemWindows(false)
            window.insetsController?.hide(WindowInsets.Type.systemBars())
            window.insetsController?.systemBarsBehavior = WindowInsetsController.BEHAVIOR_SHOW_TRANSIENT_BARS_BY_SWIPE
        } else {
            @Suppress("DEPRECATION")
            window.decorView.systemUiVisibility = View.SYSTEM_UI_FLAG_FULLSCREEN or View.SYSTEM_UI_FLAG_HIDE_NAVIGATION or View.SYSTEM_UI_FLAG_IMMERSIVE_STICKY or View.SYSTEM_UI_FLAG_LAYOUT_STABLE
        }
        window.attributes = window.attributes.also { it.layoutInDisplayCutoutMode = WindowManager.LayoutParams.LAYOUT_IN_DISPLAY_CUTOUT_MODE_SHORT_EDGES }
    }
}
