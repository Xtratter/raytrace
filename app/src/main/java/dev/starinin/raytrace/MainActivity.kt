package dev.starinin.raytrace

import android.app.Activity
import android.content.res.Configuration
import android.os.Build
import android.os.Bundle
import android.os.Handler
import android.os.Looper
import android.view.*
import android.widget.FrameLayout
import io.github.xtratter.uikit.Expressive
import io.github.xtratter.uikit.Haptics
import io.github.xtratter.uikit.M3
import io.github.xtratter.uikit.M3Widgets

class MainActivity : Activity() {
    lateinit var settings: Settings
    private lateinit var root: FrameLayout
    private lateinit var render: RenderView
    private lateinit var hud: Hud
    private lateinit var sheet: SettingsSheet
    private val handler = Handler(Looper.getMainLooper())
    private val tick = object : Runnable {
        override fun run() { hud.update(Native.stats(), settings.get(Ids.HUD).toInt()); handler.postDelayed(this, 500) }
    }

    override fun onCreate(b: Bundle?) {
        super.onCreate(b)
        val sp = getSharedPreferences("prefs", MODE_PRIVATE)
        settings = Settings(object : KeyValue {
            override fun getFloat(k: String, d: Float) = sp.getFloat(k, d)
            override fun putFloat(k: String, v: Float) { sp.edit().putFloat(k, v).apply() }
        }) { id, v -> Native.setParam(id, v) }
        intent.getStringExtra("dbg")?.split(',')?.forEach {
            val (i, v) = it.split('='); settings.set(i.toInt(), v.toFloat())
        }
        Haptics.init(this, sp)
        Haptics.onTouch = { v, e -> Expressive.morph(v, e) }
        window.addFlags(WindowManager.LayoutParams.FLAG_KEEP_SCREEN_ON)
        // once only: re-assigning window attributes on each focus change fires surfaceChanged again
        window.attributes = window.attributes.also { it.layoutInDisplayCutoutMode = WindowManager.LayoutParams.LAYOUT_IN_DISPLAY_CUTOUT_MODE_SHORT_EDGES }
        root = FrameLayout(this)
        render = RenderView(this, settings)
        root.addView(render, FrameLayout.LayoutParams(-1, -1))
        buildOverlay()
        setContentView(root)
        intent.getIntExtra("tab", -1).let { if (it >= 0) sheet.showTab(it) }   // debug: open the sheet at a tab
    }

    private fun applyTheme() {
        val modes = M3.Mode.values()
        M3.apply(this, modes[settings.get(Ids.THEME).toInt().coerceIn(0, modes.size - 1)], translucent = true)
    }

    /** HUD, gear button and settings sheet over the render view; rebuilt on a theme change (the engine keeps running). */
    private fun buildOverlay() {
        applyTheme()
        while (root.childCount > 1) root.removeViewAt(1)
        val dp = resources.displayMetrics.density
        fun px(v: Float) = (v * dp).toInt()
        hud = Hud(this)
        hud.update(Native.stats(), settings.get(Ids.HUD).toInt())
        root.addView(hud, FrameLayout.LayoutParams(-2, -2, Gravity.TOP or Gravity.START).apply { setMargins(px(16f), px(40f), 0, 0) })
        val gear = M3Widgets.button(this, "⚙︎", M3Widgets.ButtonKind.TONAL) { if (sheet.isOpen) sheet.hide() else sheet.show() }
        gear.setPadding(0, 0, 0, 0); gear.textSize = 22f
        root.addView(gear, FrameLayout.LayoutParams(px(52f), px(52f), Gravity.TOP or Gravity.END).apply { setMargins(0, px(34f), px(16f), 0) })
        sheet = SettingsSheet(this, settings) { val t = sheet.tab; buildOverlay(); sheet.showTab(t) }
        root.addView(sheet, FrameLayout.LayoutParams(-1, -1))
    }

    override fun onConfigurationChanged(c: Configuration) {
        super.onConfigurationChanged(c)
        // "System" theme follows Android's dark mode; the activity is not recreated on uiMode changes
        if (settings.get(Ids.THEME) == 0f && !M3.isCurrent(this, M3.Mode.SYSTEM, true)) {
            val open = sheet.isOpen; val t = sheet.tab
            buildOverlay(); if (open) sheet.showTab(t)
        }
    }

    override fun onResume() { super.onResume(); handler.post(tick) }
    override fun onPause() { handler.removeCallbacks(tick); super.onPause() }
    @Deprecated("Deprecated in Java")
    override fun onBackPressed() { if (sheet.isOpen) sheet.hide() else super.onBackPressed() }

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
    }
}
