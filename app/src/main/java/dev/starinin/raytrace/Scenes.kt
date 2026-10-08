package dev.starinin.raytrace

import android.content.Context
import java.io.File

/** Built-in scenes (ids 0..2 of the `scene` setting) and user scene files (id 3): JSON in the app's external files dir. */
object Scenes {
    val NAMES get() = listOf(t("Classic", "Классика"), t("Sun room", "Солнечная комната"), t("Materials", "Витрина материалов"))

    fun dir(ctx: Context): File = File(ctx.getExternalFilesDir(null), "scenes").also { it.mkdirs() }

    fun files(ctx: Context): List<File> =
        dir(ctx).listFiles { f -> f.isFile && f.name.endsWith(".json", ignoreCase = true) }?.sortedBy { it.name.lowercase() } ?: emptyList()

    fun currentFile(ctx: Context): String? = ctx.getSharedPreferences("prefs", Context.MODE_PRIVATE).getString("scene_file", null)

    /** Loads [f] into the engine and selects it; returns null on success or an error message. */
    fun load(ctx: Context, s: Settings, f: File): String? {
        val err = try { Native.loadScene(f.readText()) } catch (e: Exception) { e.message ?: "read error" }
        if (err.isNotEmpty()) return "${f.name}: $err"
        ctx.getSharedPreferences("prefs", Context.MODE_PRIVATE).edit().putString("scene_file", f.name).apply()
        s.set(Ids.SCENE, 3f)
        return null
    }

    /** After the engine starts: re-load the saved scene file when "file" was the selected scene (falls back to the sun room). */
    fun restore(ctx: Context, s: Settings) {
        if (s.get(Ids.SCENE) < 2.5f) return
        val f = currentFile(ctx)?.let { File(dir(ctx), it) }
        if (f == null || !f.isFile || load(ctx, s, f) != null) s.set(Ids.SCENE, 1f)
    }
}
