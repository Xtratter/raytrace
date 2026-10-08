package dev.starinin.raytrace

import org.json.JSONArray
import org.junit.Assert.*
import org.junit.Test
import java.io.File

class FakeKv(val m: MutableMap<String, Float> = mutableMapOf()) : KeyValue {
    override fun getFloat(k: String, d: Float) = m[k] ?: d
    override fun putFloat(k: String, v: Float) { m[k] = v }
}

class SettingsTest {
    private fun json() = JSONArray(File("../params.json").readText())

    @Test fun defsMatchSharedJson() {
        val j = json()
        assertEquals(j.length(), Settings.DEFS.size)
        for (i in 0 until j.length()) {
            val e = j.getJSONObject(i)
            val d = Settings.DEFS.first { it.id == e.getInt("id") }
            assertEquals(e.getString("key"), d.key)
            assertEquals(e.getDouble("default").toFloat(), d.default)
            assertEquals(e.getDouble("min").toFloat(), d.min)
            assertEquals(e.getDouble("max").toFloat(), d.max)
            assertEquals(e.getBoolean("native"), d.native)
        }
    }

    @Test fun missingKeysGiveDefaults() {
        val s = Settings(FakeKv()) { _, _ -> }
        assertEquals(60f, s.get(Ids.TARGET_FPS))
        assertEquals(1f, s.get(Ids.PRESET))
    }

    @Test fun setClampsPersistsAndPushesNativeOnly() {
        val kv = FakeKv(); val pushed = mutableListOf<Pair<Int, Float>>()
        val s = Settings(kv) { i, v -> pushed += i to v }
        s.set(Ids.BOUNCES, 99f)
        assertEquals(9f, s.get(Ids.BOUNCES)); assertEquals(9f, kv.m["bounces"])
        assertEquals(listOf(Ids.BOUNCES to 9f), pushed)
        pushed.clear(); s.set(Ids.THEME, 2f)
        assertTrue(pushed.isEmpty())
    }

    @Test fun unknownIdIgnoredAndUnknownStoredKeysIgnored() {
        val kv = FakeKv(mutableMapOf("future_key" to 5f))
        val s = Settings(kv) { _, _ -> }
        s.set(12345, 1f)
        assertFalse(kv.m.containsKey("12345"))
    }

    @Test fun pushAllSendsEveryNativeParam() {
        val pushed = mutableSetOf<Int>()
        Settings(FakeKv()) { i, _ -> pushed += i }.pushAll()
        assertEquals(33, pushed.size)
        assertTrue(100 !in pushed)
    }

    @Test fun resetRestoresDefaults() {
        val s = Settings(FakeKv()) { _, _ -> }
        s.set(Ids.BOUNCES, 2f); s.reset()
        assertEquals(4f, s.get(Ids.BOUNCES))
    }
}
