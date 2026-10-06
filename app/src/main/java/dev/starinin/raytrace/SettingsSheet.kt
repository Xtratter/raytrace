package dev.starinin.raytrace

import android.content.Context
import android.view.Gravity
import android.view.MotionEvent
import android.widget.FrameLayout
import android.widget.HorizontalScrollView
import android.widget.LinearLayout
import android.widget.ScrollView
import android.widget.TextView
import io.github.xtratter.uikit.EdgeBlur
import io.github.xtratter.uikit.Haptics
import io.github.xtratter.uikit.M3
import io.github.xtratter.uikit.M3Surface
import io.github.xtratter.uikit.M3Widgets

/**
 * Bottom sheet with all settings in five tabs. Covers the bottom ~56 % only: the scene stays visible above it, and
 * touches outside the panel fall through to the render view underneath (camera gestures keep working).
 */
class SettingsSheet(ctx: Context, private val s: Settings, private val onTheme: () -> Unit) : FrameLayout(ctx) {
    private val dp = ctx.resources.displayMetrics.density
    private fun px(v: Float) = (v * dp).toInt()
    private val panel = LinearLayout(ctx).apply { orientation = LinearLayout.VERTICAL; isClickable = true }
    private val tabRow = LinearLayout(ctx)
    private val scroll = ScrollView(ctx).apply { isVerticalScrollBarEnabled = false }
    private val content = LinearLayout(ctx).apply { orientation = LinearLayout.VERTICAL; setPadding(0, px(4f), 0, px(24f)) }   // rows pad themselves: chip rows scroll edge to edge
    var tab = 0; private set
    /** Nearly opaque: text stays readable over a bright scene. */
    private val fill = M3.withAlpha(M3.surface, 0.94f)
    /** True while a hide animation runs: the sheet already counts as closed (the gear toggles correctly). */
    private var closing = false
    val isOpen get() = visibility == VISIBLE && !closing
    /** True during the hide animation (panel still visible, sliding out). */
    val isClosing get() = closing

    private val tabs get() = listOf(t("Quality", "Качество"), t("Smoothing", "Сглаживание"), t("Light", "Свет"), t("Scene", "Сцена"), t("App", "Прил."))

    init {
        visibility = GONE
        panel.background = M3Surface(ctx, 28f, fill, tonal = false)
        panel.clipToOutline = true
        panel.addView(HorizontalScrollView(ctx).apply { isHorizontalScrollBarEnabled = false; addView(tabRow) },
            LinearLayout.LayoutParams(-1, -2).apply { topMargin = px(14f); bottomMargin = px(4f) })
        tabRow.setPadding(px(14f), 0, px(6f), 0)
        scroll.addView(content)
        panel.addView(scroll, LinearLayout.LayoutParams(-1, 0, 1f))
        addView(panel, LayoutParams(-1, -1, Gravity.BOTTOM or Gravity.CENTER_HORIZONTAL).apply { setMargins(px(8f), 0, px(8f), px(8f)) })
        EdgeBlur.wrap(scroll, 20f, fill)
        rebuild()
    }

    /** Panel size follows the window (the activity handles rotation itself): ~56 % high, at most 520 dp wide. */
    override fun onMeasure(wSpec: Int, hSpec: Int) {
        val lp = panel.layoutParams as LayoutParams
        lp.width = minOf(MeasureSpec.getSize(wSpec) - px(16f), px(520f))
        lp.height = (MeasureSpec.getSize(hSpec) * 0.56f).toInt()
        super.onMeasure(wSpec, hSpec)
    }

    /** Only the panel takes touches; everything else goes to the views below (the scene). */
    override fun dispatchTouchEvent(e: MotionEvent): Boolean {
        if (e.actionMasked == MotionEvent.ACTION_DOWN) {
            val top = panel.top + panel.translationY
            if (e.x < panel.left || e.x > panel.right || e.y < top || e.y > top + panel.height) return false
        }
        return super.dispatchTouchEvent(e)
    }

    /** Opens fully; called during a hide animation it cancels the hide and slides back up from where the panel is. */
    fun show(animate: Boolean = true) {
        val wasClosed = visibility != VISIBLE
        closing = false
        panel.animate().cancel()   // drops a pending hide; its end action also checks [closing]
        visibility = VISIBLE
        if (!animate) { panel.translationY = 0f; return }
        if (wasClosed) panel.translationY = px(400f).toFloat()
        panel.animate().translationY(0f).setDuration(220).withEndAction(null).start()
        Haptics.play(Haptics.Kind.OPEN)
    }

    /** No-op when already closed or closing. */
    fun hide() {
        if (!isOpen) return
        closing = true
        panel.animate().cancel()
        panel.animate().translationY(px(400f).toFloat()).setDuration(180)
            .withEndAction { if (closing) { closing = false; visibility = GONE } }.start()
        Haptics.play(Haptics.Kind.CLOSE)
    }

    /** Open at tab [i] without animation (theme rebuild, debug launches). */
    fun showTab(i: Int) { tab = i.coerceIn(0, 4); rebuild(); show(animate = false) }

    private fun rebuild() {
        tabRow.removeAllViews()
        tabs.forEachIndexed { i, name ->
            tabRow.addView(M3Widgets.chip(context, name, i == tab) { tab = i; rebuild(); scroll.scrollTo(0, 0) }, chipLp())
        }
        content.removeAllViews()
        when (tab) {
            0 -> quality(); 1 -> smoothing(); 2 -> light(); 3 -> scene(); else -> app()
        }
    }

    // ---- building blocks -------------------------------------------------------------------------------------------
    private fun chipLp() = LinearLayout.LayoutParams(-2, px(36f)).apply { marginEnd = px(8f) }

    private fun label(text: String) = TextView(context).apply {
        this.text = text; textSize = 13f; setTextColor(M3.TEXT2); typeface = M3.medium
        setPadding(px(20f), px(14f), px(16f), px(6f))
    }

    private fun set(id: Int, v: Float, rebuildAfter: Boolean = false) {
        s.set(id, v); Presets.touch(s, id)
        if (id == Ids.THEME) { onTheme(); return }   // the whole overlay is rebuilt in the new colours
        if (rebuildAfter) rebuild()
    }

    private fun chipRow(chips: List<TextView>) {
        val row = LinearLayout(context)
        chips.forEach { row.addView(it, chipLp()) }
        content.addView(HorizontalScrollView(context).apply {
            isHorizontalScrollBarEnabled = false; clipToPadding = false; setPadding(px(16f), 0, px(8f), 0); addView(row)
        })
    }

    private fun chips(title: String, id: Int, opts: List<Pair<String, Float>>) {
        content.addView(label(title))
        chipRow(opts.map { (name, v) -> M3Widgets.chip(context, name, s.get(id) == v) { set(id, v, true) } })
    }

    private fun toggle(title: String, id: Int) =
        content.addView(M3Widgets.switchRow(context, title, s.on(id)) { set(id, if (it) 1f else 0f) }.apply { setPadding(px(16f), 0, px(12f), 0) })

    private fun stepper(title: String, id: Int, step: Float, fmt: (Float) -> String) {
        val d = Settings.DEFS.first { it.id == id }
        val row = LinearLayout(context).apply { gravity = Gravity.CENTER_VERTICAL; minimumHeight = px(56f); setPadding(px(16f), 0, px(16f), 0) }
        row.addView(TextView(context).apply { text = title; textSize = 16f; setTextColor(M3.TEXT) }, LinearLayout.LayoutParams(0, -2, 1f))
        val value = TextView(context).apply {
            textSize = 15f; setTextColor(M3.TEXT); typeface = M3.medium; gravity = Gravity.CENTER; minWidth = px(68f)
            fontFeatureSettings = "tnum"; text = fmt(s.get(id))
        }
        fun bump(dir: Float) {
            set(id, (s.get(id) + dir * step).coerceIn(d.min, d.max))
            value.text = fmt(s.get(id))
            if (tab == 0 && id in Presets.CONTROLLED) rebuild()   // the preset row above switches to "Custom"
        }
        row.addView(M3Widgets.button(context, "−", M3Widgets.ButtonKind.TONAL) { bump(-1f) }, LinearLayout.LayoutParams(px(52f), px(44f)))
        row.addView(value)
        row.addView(M3Widgets.button(context, "+", M3Widgets.ButtonKind.TONAL) { bump(1f) }, LinearLayout.LayoutParams(px(52f), px(44f)))
        content.addView(row)
    }

    private val onOff get() = listOf(t("Off", "Выкл") to 0f, t("On", "Вкл") to 1f)

    // ---- tabs ------------------------------------------------------------------------------------------------------
    private fun quality() {
        // preset chips apply a whole preset; "Custom" is only an indicator of hand-edited values
        content.addView(label(t("Preset", "Пресет")))
        chipRow((Presets.NAMES + t("Custom", "Свой")).mapIndexed { i, n ->
            M3Widgets.chip(context, n, s.get(Ids.PRESET).toInt() == i) { if (i < 3) Presets.apply(s, i); rebuild() }
        })
        chips(t("Mode", "Режим"), Ids.MODE, listOf(t("Hybrid", "Гибрид") to 0f, t("Path tracing", "Трассировка путей") to 1f))
        chips(t("Render scale", "Масштаб рендера"), Ids.SCALE_IDX, listOf("0.25×" to 0f, "0.33×" to 1f, "0.5×" to 2f, "0.75×" to 3f, "1.0×" to 4f))
        chips(t("Adaptive resolution", "Адаптивное разрешение"), Ids.ADAPTIVE, onOff)
        chips(t("Target FPS", "Целевой FPS"), Ids.TARGET_FPS, listOf("30" to 30f, "45" to 45f, "60" to 60f, "90" to 90f))
        stepper(t("Bounces", "Отражения луча"), Ids.BOUNCES, 1f) { it.toInt().toString() }
        stepper(t("Samples per pixel", "Сэмплов на пиксель"), Ids.SPP, 1f) { it.toInt().toString() }
    }

    private fun smoothing() {
        toggle(t("Temporal smoothing", "Временное сглаживание"), Ids.TEMPORAL)
        stepper(t("Smoothing strength", "Сила сглаживания"), Ids.STRENGTH, 1f) { it.toInt().toString() }
        chips(t("Denoiser passes", "Проходы денойзера"), Ids.DENOISE, listOf("0" to 0f, "1" to 1f, "2" to 2f, "3" to 3f))
        chips(t("Sharpen", "Резкость"), Ids.SHARPEN, listOf(t("Off", "Выкл") to 0f, t("Low", "Низкая") to 1f, t("Medium", "Средняя") to 2f, t("High", "Высокая") to 3f))
        toggle(t("Checkerboard rendering", "Шахматный рендер"), Ids.CHECKER)
    }

    private fun light() {
        toggle(t("Soft shadows", "Мягкие тени"), Ids.SHADOWS)
        toggle(t("Global illumination", "Глобальное освещение"), Ids.GI)
        toggle(t("Caustics", "Каустики"), Ids.CAUSTICS)
        toggle(t("Reflections & refraction", "Отражения и преломление"), Ids.REFLECTIONS)
        stepper(t("Light intensity", "Яркость ламп"), Ids.LIGHT, 1f) { listOf("0.5×", "0.75×", "1×", "1.5×", "2×")[it.toInt() - 1] }
        val names = listOf(t("Warm", "Тёплый"), t("White", "Белый"), t("Cyan", "Голубой"), t("Pink", "Розовый"), t("Green", "Зелёный"), t("Orange", "Оранжевый"))
        val dots = listOf(0xFFFFC27A, 0xFFFFFFFF, 0xFF7AD0FF, 0xFFFF7AE0, 0xFF7AFF9A, 0xFFFF9A3A).map { it.toInt() }
        for ((title, id) in listOf(t("Light A colour", "Цвет лампы A") to Ids.COL_A, t("Light B colour", "Цвет лампы B") to Ids.COL_B)) {
            content.addView(label(title))
            chipRow(names.mapIndexed { i, n -> M3Widgets.chip(context, n, s.get(id).toInt() == i, dots[i]) { set(id, i.toFloat(), true) } })
        }
    }

    private fun scene() {
        toggle(t("Animation", "Анимация"), Ids.ANIM)
        chips(t("Auto-orbit", "Автовращение"), Ids.ORBIT, listOf(t("Off", "Выкл") to 0f, t("Slow", "Медленно") to 1f, t("Fast", "Быстро") to 2f))
        stepper(t("Field of view", "Угол обзора"), Ids.FOV, 5f) { "${it.toInt()}°" }
        stepper(t("Exposure", "Экспозиция"), Ids.EXPOSURE, 1f) { "%+.1f EV".format(it * 0.5f) }
        chips(t("Tonemap", "Тональная кривая"), Ids.TONEMAP, listOf("ACES" to 0f, "Reinhard" to 1f, t("None", "Нет") to 2f))
        chips(t("Sky", "Небо"), Ids.SKY, listOf(t("Day", "День") to 0f, t("Dusk", "Закат") to 1f, t("Night", "Ночь") to 2f))
    }

    private fun app() {
        chips(t("Theme", "Тема"), Ids.THEME, listOf(t("System", "Система") to 0f, t("Light", "Светлая") to 1f, t("Dark", "Тёмная") to 2f, t("Graphite", "Графит") to 3f, "AMOLED" to 4f))
        chips("HUD", Ids.HUD, listOf(t("Off", "Выкл") to 0f, "FPS" to 1f, t("Full", "Полный") to 2f))
        toggle(t("Limit to display refresh", "Лимит по частоте экрана"), Ids.FRAME_LIMIT)
        content.addView(M3Widgets.button(context, t("Reset to defaults", "Сбросить настройки"), M3Widgets.ButtonKind.DANGER) {
            val theme = s.get(Ids.THEME)
            s.reset(); Presets.apply(s, 1)
            if (s.get(Ids.THEME) != theme) onTheme() else rebuild()
        }, LinearLayout.LayoutParams(-1, px(52f)).apply { topMargin = px(20f); marginStart = px(16f); marginEnd = px(16f) })
    }
}
