package dev.starinin.raytrace

import android.app.Activity
import android.os.Bundle
import android.widget.TextView

class MainActivity : Activity() {
    override fun onCreate(b: Bundle?) {
        super.onCreate(b)
        setContentView(TextView(this).apply { text = "Raytrace" })
    }
}
