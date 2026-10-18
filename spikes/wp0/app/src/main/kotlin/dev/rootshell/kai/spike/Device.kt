package dev.rootshell.kai.spike

import android.app.Activity
import android.content.Context
import android.content.res.Configuration
import android.os.Build
import android.system.Os
import android.system.OsConstants
import android.view.WindowManager

/** Facts about the phone and its windows that every point records beside its numbers. */
object Device {
    /** A system property through `getprop` (no hidden API); empty when unset or unreadable. */
    fun prop(name: String): String = runCatching {
        val p = ProcessBuilder("getprop", name).redirectErrorStream(true).start()
        p.inputStream.bufferedReader().use { it.readText().trim() }.also { p.waitFor() }
    }.getOrDefault("")

    fun pageSize(): Long = Os.sysconf(OsConstants._SC_PAGESIZE)

    fun facts(context: Context): Map<String, Any?> = mapOf(
        "manufacturer" to Build.MANUFACTURER,
        "model" to Build.MODEL,
        "device" to Build.DEVICE,
        "android_release" to Build.VERSION.RELEASE,
        "sdk_int" to Build.VERSION.SDK_INT,
        "security_patch" to Build.VERSION.SECURITY_PATCH,
        "build_display" to Build.DISPLAY,
        "fingerprint" to Build.FINGERPRINT,
        "oneui" to prop("ro.build.version.oneui"),
        "lineage" to prop("ro.lineage.version"),
        "first_api_level" to prop("ro.product.first_api_level"),
        "page_size" to pageSize(),
        "abis" to Build.SUPPORTED_ABIS.toList(),
        "app_version" to runCatching {
            context.packageManager.getPackageInfo(context.packageName, 0).versionName
        }.getOrNull(),
    )

    /** A foldable's screen by its smallest width: the cover screen is under 600 dp, the inner one not. */
    fun posture(configuration: Configuration): String =
        if (configuration.smallestScreenWidthDp < 600) "cover" else "inner"

    fun posture(context: Context): String = posture(context.resources.configuration)
}

/** A window's size as WindowMetrics gives it, in px and dp, with its size classes (§10.6). */
data class WindowFacts(
    val widthPx: Int,
    val heightPx: Int,
    val density: Float,
    val widthDp: Int,
    val heightDp: Int,
    val smallestDp: Int,
    val widthClass: String,
    val heightClass: String,
    val posture: String,
) {
    fun toMap(): Map<String, Any?> = mapOf(
        "width_px" to widthPx, "height_px" to heightPx, "density" to density.toDouble(),
        "width_dp" to widthDp, "height_dp" to heightDp, "smallest_dp" to smallestDp,
        "width_class" to widthClass, "height_class" to heightClass, "posture" to posture,
    )

    override fun toString() =
        "${widthPx}×$heightPx px, ${widthDp}×$heightDp dp (density $density), $widthClass × $heightClass, $posture"

    companion object {
        fun widthClass(dp: Int) = when {
            dp < 600 -> "compact"
            dp < 840 -> "medium"
            else -> "expanded"
        }

        fun heightClass(dp: Int) = when {
            dp < 480 -> "compact"
            dp < 900 -> "medium"
            else -> "expanded"
        }

        /** The current window metrics of `wm` (an activity's or a session window's manager). */
        fun of(wm: WindowManager, configuration: Configuration, density: Float): WindowFacts {
            val b = wm.currentWindowMetrics.bounds
            val w = (b.width() / density).toInt()
            val h = (b.height() / density).toInt()
            return WindowFacts(
                b.width(), b.height(), density, w, h, minOf(w, h),
                widthClass(w), heightClass(h), Device.posture(configuration),
            )
        }

        fun maximum(wm: WindowManager, configuration: Configuration, density: Float): WindowFacts {
            val b = wm.maximumWindowMetrics.bounds
            val w = (b.width() / density).toInt()
            val h = (b.height() / density).toInt()
            return WindowFacts(
                b.width(), b.height(), density, w, h, minOf(w, h),
                widthClass(w), heightClass(h), Device.posture(configuration),
            )
        }

        fun of(activity: Activity): WindowFacts =
            of(activity.windowManager, activity.resources.configuration, activity.resources.displayMetrics.density)
    }
}
