package dev.starinin.raytrace

interface KeyValue {
    fun getFloat(k: String, d: Float): Float
    fun putFloat(k: String, v: Float)
}

object Ids {
    const val MODE = 0; const val SCALE_IDX = 1; const val ADAPTIVE = 2; const val TARGET_FPS = 3
    const val BOUNCES = 4; const val SPP = 5; const val TEMPORAL = 6; const val STRENGTH = 7
    const val DENOISE = 8; const val SHARPEN = 9; const val CHECKER = 10; const val SHADOWS = 11
    const val GI = 12; const val CAUSTICS = 13; const val REFLECTIONS = 14; const val LIGHT = 15
    const val COL_A = 16; const val COL_B = 17; const val ANIM = 18; const val ORBIT = 19
    const val FOV = 20; const val EXPOSURE = 21; const val TONEMAP = 22; const val SKY = 23
    const val HUD = 24; const val FRAME_LIMIT = 25
    const val CAM_MODE = 26; const val STICKS = 27; const val MOVE_SPEED = 28; const val LOOK_SPEED = 29
    const val THEME = 100; const val PRESET = 102
}

class Settings(private val kv: KeyValue, private val push: (Int, Float) -> Unit) {
    class Def(val id: Int, val key: String, val default: Float, val min: Float, val max: Float, val native: Boolean)

    companion object {
        private fun d(id: Int, key: String, def: Float, min: Float, max: Float, native: Boolean = true) =
            Def(id, key, def, min, max, native)

        /** Must match ../params.json (checked by SettingsTest). */
        val DEFS = listOf(
            d(0, "mode", 0f, 0f, 1f), d(1, "scale_idx", 1f, 0f, 4f), d(2, "adaptive", 1f, 0f, 1f),
            d(3, "target_fps", 60f, 30f, 90f), d(4, "bounces", 6f, 1f, 9f), d(5, "spp", 1f, 1f, 4f),
            d(6, "temporal", 1f, 0f, 1f), d(7, "strength", 3f, 1f, 5f), d(8, "denoise", 1f, 0f, 3f),
            d(9, "sharpen", 1f, 0f, 3f), d(10, "checker", 0f, 0f, 1f), d(11, "shadows", 1f, 0f, 1f),
            d(12, "gi", 1f, 0f, 1f), d(13, "caustics", 1f, 0f, 1f), d(14, "reflections", 1f, 0f, 1f),
            d(15, "light", 3f, 1f, 5f), d(16, "col_a", 0f, 0f, 5f), d(17, "col_b", 0f, 0f, 5f),
            d(18, "anim", 1f, 0f, 1f), d(19, "orbit", 0f, 0f, 2f), d(20, "fov", 60f, 40f, 90f),
            d(21, "exposure", 0f, -4f, 4f), d(22, "tonemap", 0f, 0f, 2f), d(23, "sky", 0f, 0f, 2f),
            d(24, "hud", 1f, 0f, 2f), d(25, "frame_limit", 1f, 0f, 1f),
            d(26, "cam_mode", 0f, 0f, 1f), d(27, "sticks", 1f, 0f, 1f),
            d(28, "move_speed", 3f, 1f, 5f), d(29, "look_speed", 3f, 1f, 5f),
            d(100, "theme", 0f, 0f, 4f, false), d(102, "preset", 1f, 0f, 3f, false),
        )
    }

    private val byId = DEFS.associateBy { it.id }
    private val values = HashMap<Int, Float>().also { m ->
        for (d in DEFS) m[d.id] = kv.getFloat(d.key, d.default).let { if (it.isFinite()) it.coerceIn(d.min, d.max) else d.default }
    }

    fun get(id: Int): Float = values[id] ?: 0f
    fun on(id: Int) = get(id) > 0.5f

    fun set(id: Int, v: Float) {
        val d = byId[id] ?: return
        if (!v.isFinite()) return
        val c = v.coerceIn(d.min, d.max)
        values[id] = c
        kv.putFloat(d.key, c)
        if (d.native) push(id, c)
    }

    fun pushAll() { for (d in DEFS) if (d.native) push(d.id, get(d.id)) }

    fun reset() { for (d in DEFS) set(d.id, d.default) }
}
