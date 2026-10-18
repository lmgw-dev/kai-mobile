package dev.rootshell.kai.spike

import android.app.Application
import android.app.NotificationChannel
import android.app.NotificationManager
import android.content.Context
import android.os.Process
import android.os.SystemClock
import androidx.core.content.edit

/**
 * The spike's process. Every start is logged (`app.start`), so a cold invocation (a) or a process
 * the system restarted shows in the log. The (e) probe's process (`:ffiprobe`) only sets up the
 * log: it must stay cold for the core.
 */
class SpikeApp : Application() {
    override fun onCreate() {
        super.onCreate()
        val probe = getProcessName().endsWith(":ffiprobe")
        SpikeLog.init(this, if (probe) "ffiprobe" else "main")
        if (probe) return
        createChannels(this)
        // Off the main thread: getprop runs as processes, and a cold invocation (a) must not wait for them.
        val age = processAgeMs()
        Thread({ SpikeLog.event("app", "start", Device.facts(this) + mapOf("process_age_ms" to age)) }, "spike-start").start()
    }

    companion object {
        const val CHANNEL_STATUS = "status"
        const val CHANNEL_CHECK = "selfcheck"

        /** Visible activities of this process (b: whether a type change came from a visible app). */
        @Volatile var visibleActivities = 0

        fun processAgeMs(): Long = SystemClock.elapsedRealtime() - Process.getStartElapsedRealtime()

        private fun createChannels(context: Context) {
            val nm = context.getSystemService(NotificationManager::class.java)
            // As Kai's (design §8.6, §12.1): low, silent, the service's one notification.
            nm.createNotificationChannel(
                NotificationChannel(CHANNEL_STATUS, "Status (the service)", NotificationManager.IMPORTANCE_LOW).apply {
                    setSound(null, null)
                    enableVibration(false)
                },
            )
            nm.createNotificationChannel(
                NotificationChannel(CHANNEL_CHECK, "Self-check", NotificationManager.IMPORTANCE_DEFAULT),
            )
        }
    }
}

/** The spike's few settings (SharedPreferences "spike"). */
object Prefs {
    private fun p(context: Context) = context.getSharedPreferences("spike", Context.MODE_PRIVATE)

    /** (a): setDisabledShowContext(SHOW_WITH_ASSIST | SHOW_WITH_SCREENSHOT) in onReady (design §9.2). */
    fun disableShowContext(context: Context) = p(context).getBoolean("disable_show_context", true)
    fun setDisableShowContext(context: Context, on: Boolean) = p(context).edit { putBoolean("disable_show_context", on) }

    /** (b): add the microphone type as soon as the overlay shows, as a real call would. */
    fun micOnShow(context: Context) = p(context).getBoolean("mic_on_show", false)
    fun setMicOnShow(context: Context, on: Boolean) = p(context).edit { putBoolean("mic_on_show", on) }

    /** (b): the host (or host:port for TCP) the ping loop measures. */
    fun pingHost(context: Context) = p(context).getString("ping_host", "").orEmpty()
    fun setPingHost(context: Context, host: String) = p(context).edit { putString("ping_host", host.trim()) }

    /** (b): after how many minutes a capture stops by itself (0 = only by hand). */
    fun captureMinutes(context: Context) = p(context).getInt("capture_minutes", 12)
    fun setCaptureMinutes(context: Context, minutes: Int) = p(context).edit { putInt("capture_minutes", minutes) }
}
