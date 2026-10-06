package dev.starinin.raytrace

import java.util.Locale

/** Bilingual UI string: Russian on a Russian-locale device, English otherwise. */
fun t(en: String, ru: String) = if (Locale.getDefault().language == "ru") ru else en
