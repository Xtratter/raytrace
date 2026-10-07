package dev.starinin.raytrace

import android.content.Context
import android.view.Gravity
import android.widget.TextView
import io.github.xtratter.uikit.M3
import io.github.xtratter.uikit.M3Surface

/** Status island: fps · gpu ms · resolution · mode. Level 0 hidden, 1 fps only, 2 full. */
class Hud(ctx: Context) : TextView(ctx) {
    init {
        val dp = ctx.resources.displayMetrics.density
        textSize = 13f; typeface = M3.medium; gravity = Gravity.CENTER
        fontFeatureSettings = "tnum"   // equal-width digits: the island does not jitter
        setPadding((14 * dp).toInt(), (7 * dp).toInt(), (14 * dp).toInt(), (7 * dp).toInt())
        setTextColor(M3.TEXT)
        background = M3Surface(ctx, 20f, M3.surface)
        text = "— fps"
    }

    fun update(st: FloatArray, level: Int) {
        visibility = if (level == 0) GONE else VISIBLE
        if (level == 0 || st.size < 6) return
        val fps = "%.0f fps".format(st[0])
        val line = if (level == 1) fps else
            "$fps · ${"%.1f".format(st[1])} ms · ${st[2].toInt()}×${st[3].toInt()} · ${if (st[5] > 0.5f) t("path", "путь") else t("hybrid", "гибрид")}"
        // st[8..16]: per-pass GPU ms (trace, gi_trace, gi_temporal, gi_atrous, temporal, atrous, composite, present); 0 when unsupported
        text = if (level == 2 && st.size >= 16 && (8 until 16).any { st[it] > 0f }) {
            val n = listOf("tr", "gi", "gt", "ga", "tm", "at", "cm", "pr")
            line + "\n" + n.indices.joinToString(" · ") { "${n[it]} ${"%.1f".format(st[8 + it])}" }
        } else line
    }
}
