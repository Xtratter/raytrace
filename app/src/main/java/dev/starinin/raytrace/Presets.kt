package dev.starinin.raytrace

object Presets {
    val NAMES get() = listOf(t("Performance", "Скорость"), t("Balanced", "Баланс"), t("Quality", "Качество"))

    /** Params a preset sets, in the column order of [TABLE]. Scene and app settings are never touched. */
    private val ORDER = listOf(Ids.SCALE_IDX, Ids.ADAPTIVE, Ids.TARGET_FPS, Ids.BOUNCES, Ids.SPP, Ids.TEMPORAL, Ids.STRENGTH,
        Ids.DENOISE, Ids.SHARPEN, Ids.CHECKER, Ids.SHADOWS, Ids.GI, Ids.CAUSTICS, Ids.REFLECTIONS)
    val CONTROLLED = ORDER.toSet()

    private val TABLE = listOf(
        // scale adaptive fps bounces spp temporal strength denoise sharpen checker shadows gi caustics reflections
        floatArrayOf(0f, 1f, 60f, 4f, 1f, 1f, 4f, 2f, 2f, 0f, 1f, 0f, 0f, 1f),   // Performance (checker off: grainier, only ~25% faster)
        floatArrayOf(1f, 1f, 60f, 6f, 1f, 1f, 3f, 1f, 1f, 0f, 1f, 1f, 1f, 1f),   // Balanced = params.json defaults
        floatArrayOf(2f, 0f, 60f, 9f, 2f, 1f, 2f, 1f, 1f, 0f, 1f, 1f, 1f, 1f),   // Quality
    )

    fun apply(s: Settings, index: Int) {
        val row = TABLE.getOrNull(index) ?: return
        ORDER.forEachIndexed { i, id -> s.set(id, row[i]) }
        s.set(Ids.PRESET, index.toFloat())
    }

    /** Call after the user edits [id] by hand: preset-controlled params make the preset "Custom". */
    fun touch(s: Settings, id: Int) { if (id in CONTROLLED) s.set(Ids.PRESET, 3f) }
}
