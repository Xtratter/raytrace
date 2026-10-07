package dev.starinin.raytrace

import org.junit.Assert.*
import org.junit.Test

class PresetsTest {
    private fun s() = Settings(FakeKv()) { _, _ -> }

    @Test fun performanceUsesLowScaleWithoutGi() {
        val st = s(); Presets.apply(st, 0)
        assertEquals(0f, st.get(Ids.SCALE_IDX)); assertEquals(0f, st.get(Ids.CHECKER))
        assertEquals(0f, st.get(Ids.GI)); assertEquals(0f, st.get(Ids.PRESET))
    }

    @Test fun qualityDisablesAdaptiveAndRaisesBounces() {
        val st = s(); Presets.apply(st, 2)
        assertEquals(0f, st.get(Ids.ADAPTIVE)); assertEquals(9f, st.get(Ids.BOUNCES)); assertEquals(2f, st.get(Ids.PRESET))
    }

    @Test fun presetDoesNotTouchSceneOrAppSettings() {
        val st = s(); st.set(Ids.EXPOSURE, 3f); st.set(Ids.THEME, 2f); st.set(Ids.COL_A, 4f)
        Presets.apply(st, 0)
        assertEquals(3f, st.get(Ids.EXPOSURE)); assertEquals(2f, st.get(Ids.THEME)); assertEquals(4f, st.get(Ids.COL_A))
    }

    @Test fun editingQualityParamMarksCustom() {
        val st = s(); Presets.apply(st, 0)
        Presets.touch(st, Ids.BOUNCES)
        assertEquals(3f, st.get(Ids.PRESET))
        Presets.touch(st, Ids.EXPOSURE)   // not a preset-controlled id: unchanged
        assertEquals(3f, st.get(Ids.PRESET))
    }

    @Test fun balancedMatchesDefaults() {
        val st = s(); Presets.apply(st, 1)
        for (d in Settings.DEFS) if (d.id in Presets.CONTROLLED) assertEquals(d.key, d.default, st.get(d.id))
    }

    @Test fun outOfRangeIndexIsNoOp() {
        val st = s(); Presets.apply(st, 0); st.set(Ids.BOUNCES, 7f)
        val before = Settings.DEFS.associate { it.id to st.get(it.id) }
        for (i in listOf(-1, 3, 99)) Presets.apply(st, i)
        for (d in Settings.DEFS) assertEquals(d.key, before[d.id], st.get(d.id))
    }

    @Test fun presetsSetGiResolution() {
        val st = s()
        Presets.apply(st, 0); assertEquals(2f, st.get(Ids.GI_RES))
        Presets.apply(st, 1); assertEquals(1f, st.get(Ids.GI_RES))
        Presets.apply(st, 2); assertEquals(0f, st.get(Ids.GI_RES))
    }

    @Test fun editingGiResolutionMarksCustom() {
        val st = s(); Presets.apply(st, 1)
        Presets.touch(st, Ids.GI_RES)
        assertEquals(3f, st.get(Ids.PRESET))
    }
}
