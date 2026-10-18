package dev.rootshell.kai.spike.service

import android.Manifest
import android.app.Notification
import android.app.NotificationManager
import android.app.PendingIntent
import android.app.Service
import android.content.Context
import android.content.Intent
import android.content.pm.PackageManager
import android.content.pm.ServiceInfo
import android.os.Build
import android.os.Handler
import android.os.IBinder
import android.os.Looper
import android.os.SystemClock
import dev.rootshell.kai.spike.Prefs
import dev.rootshell.kai.spike.R
import dev.rootshell.kai.spike.SpikeActivity
import dev.rootshell.kai.spike.SpikeApp
import dev.rootshell.kai.spike.SpikeLog
import dev.rootshell.kai.spike.assist.SpikeSession
import dev.rootshell.kai.spike.msSince
import dev.rootshell.kai.spike.r2
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow

/**
 * Kai's service as design §8.6 and §12.1 have it, for (b) and (n): a `specialUse` foreground
 * service that adds the `microphone` type for a capture (from the assistant session, a visible
 * activity, or the background as a control) and drops it after. Every `startForeground` is
 * logged with its origin, its outcome and whether an activity or the overlay was visible.
 */
class SpikeService : Service() {
    data class State(
        val running: Boolean = false,
        val types: String = "",
        val capture: Capture.Stats? = null,
        val ping: Pinger.Stats? = null,
        val lastStep: String = "",
    )

    private val main = Handler(Looper.getMainLooper())
    private var capture: Capture? = null
    private var pinger: Pinger? = null
    private var muted = false
    private val autoStop = Runnable { setMic(false, "auto_stop") }

    override fun onCreate() {
        super.onCreate()
        instance = this
        update { it.copy(running = true) }
    }

    override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int {
        val origin = intent?.getStringExtra(EXTRA_ORIGIN) ?: "system"
        when (intent?.action) {
            ACTION_MIC_ON -> {
                // specialUse first: it has no prerequisite, so the service is foreground in time
                // even when the microphone type is refused (and never while a capture holds it).
                if (capture == null) goForeground(SPECIAL_USE, origin, "start")
                setMic(true, origin)
            }
            ACTION_MIC_OFF -> setMic(false, origin)
            ACTION_MUTE -> {
                muted = !muted
                SpikeLog.event("b", "notification_action", mapOf("action" to "mute", "muted" to muted))
                refreshNotification()
            }
            ACTION_STOP -> {
                setMic(false, origin)
                stopForeground(STOP_FOREGROUND_REMOVE)
                stopSelf()
            }
            // A start while a capture runs keeps the microphone type (it would drop it otherwise).
            else -> if (capture == null) {
                goForeground(SPECIAL_USE, origin, "start")
            } else {
                SpikeLog.event("b", "start_while_capturing", mapOf("origin" to origin, "types" to typesText(foregroundServiceType)))
            }
        }
        // A spike: never restarted behind the owner's back.
        return START_NOT_STICKY
    }

    override fun onBind(intent: Intent?): IBinder? = null

    override fun onDestroy() {
        capture?.stop()
        pinger?.stop()
        main.removeCallbacks(autoStop)
        instance = null
        state.value = State()
        SpikeLog.event("b", "service_destroyed")
        super.onDestroy()
    }

    private fun typesText(types: Int): String = buildList {
        if (Build.VERSION.SDK_INT >= 34 && types and SPECIAL_USE != 0) add("specialUse")
        if (types and ServiceInfo.FOREGROUND_SERVICE_TYPE_MICROPHONE != 0) add("microphone")
    }.joinToString("|").ifEmpty { "none" }

    /** `startForeground` with `types`; whether Android took it. */
    private fun goForeground(types: Int, origin: String, step: String): Boolean {
        val t = System.nanoTime()
        val fields = mutableMapOf<String, Any?>(
            "step" to step, "origin" to origin, "types" to typesText(types),
            "activity_visible" to (SpikeApp.visibleActivities > 0), "overlay_showing" to SpikeSession.showing,
        )
        return try {
            startForeground(NOTIFICATION_ID, notification(), types)
            fields += mapOf("ok" to true, "ms" to msSince(t).r2(), "now_types" to typesText(foregroundServiceType))
            SpikeLog.event("b", "foreground", fields)
            update { it.copy(types = typesText(foregroundServiceType), lastStep = "$step from $origin: ok") }
            true
        } catch (e: Exception) {
            fields += mapOf("ok" to false, "error" to e.javaClass.name, "message" to e.message,
                "now_types" to typesText(foregroundServiceType))
            SpikeLog.event("b", "foreground", fields)
            update { it.copy(lastStep = "$step from $origin: ${e.javaClass.simpleName}") }
            false
        }
    }

    fun setMic(on: Boolean, origin: String) {
        if (Looper.myLooper() != Looper.getMainLooper()) {
            main.post { setMic(on, origin) }
            return
        }
        if (on) {
            if (capture != null) return
            if (checkSelfPermission(Manifest.permission.RECORD_AUDIO) != PackageManager.PERMISSION_GRANTED) {
                SpikeLog.event("b", "mic_refused", mapOf("origin" to origin, "why" to "RECORD_AUDIO not granted"))
                update { it.copy(lastStep = "RECORD_AUDIO is not granted") }
                return
            }
            val ok = goForeground(
                SPECIAL_USE or ServiceInfo.FOREGROUND_SERVICE_TYPE_MICROPHONE,
                origin, "mic_on",
            )
            if (!ok) return
            val c = Capture(this) { stats ->
                update { s -> s.copy(capture = stats, ping = pinger?.stats()) }
                main.post { refreshNotification() }
            }
            capture = c
            c.start(origin)
            val host = Prefs.pingHost(this)
            pinger = if (host.isNotBlank()) Pinger(this, host).also { it.start() } else null
            val minutes = Prefs.captureMinutes(this)
            if (minutes > 0) main.postDelayed(autoStop, minutes * 60_000L)
            SpikeLog.event("b", "capture_started", mapOf("origin" to origin, "ping" to host.ifBlank { null },
                "auto_stop_min" to minutes))
            update { it.copy(capture = c.stats()) }
        } else {
            main.removeCallbacks(autoStop)
            val c = capture ?: return
            capture = null
            val stats = c.stop()
            val ping = pinger?.stop()
            pinger = null
            SpikeLog.event("b", "capture_summary", stats.toMap() + mapOf("stopped_by" to origin, "ping" to ping?.toMap()))
            update { it.copy(capture = stats, ping = ping) }
            goForeground(SPECIAL_USE, origin, "mic_off")
        }
    }

    private fun refreshNotification() {
        if (instance == null) return
        getSystemService(NotificationManager::class.java).notify(NOTIFICATION_ID, notification())
    }

    private fun notification(): Notification {
        val c = capture?.stats()
        val p = pinger?.stats()
        val text = if (c != null && c.running) {
            val ping = p?.last?.let { " · ping ${"%.0f".format(it)} ms" }.orEmpty()
            "${if (muted) "muted" else "listening"} · ${c.durationMs / 1000} s · lost ${"%.0f".format(c.lostMs)} ms · gap max ${"%.0f".format(c.maxGapMs)} ms$ping"
        } else {
            "Kai spike · holding the service (no lmgw in WP0)"
        }
        fun action(action: String, code: Int) = PendingIntent.getService(
            this, code, Intent(this, SpikeService::class.java).setAction(action).putExtra(EXTRA_ORIGIN, "notification"),
            PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT,
        )
        val open = PendingIntent.getActivity(
            this, 0, Intent(this, SpikeActivity::class.java).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK),
            PendingIntent.FLAG_IMMUTABLE,
        )
        val builder = Notification.Builder(this, SpikeApp.CHANNEL_STATUS)
            .setSmallIcon(R.drawable.ic_spike_notification)
            .setContentTitle("Kai spike")
            .setContentText(text)
            .setOngoing(true)
            .setOnlyAlertOnce(true)
            .setVisibility(Notification.VISIBILITY_PUBLIC)
            .setCategory(Notification.CATEGORY_SERVICE)
            .setForegroundServiceBehavior(Notification.FOREGROUND_SERVICE_IMMEDIATE)
            .setContentIntent(open)
        if (c != null && c.running) {
            builder.addAction(Notification.Action.Builder(null, if (muted) "Unmute" else "Mute", action(ACTION_MUTE, 1)).build())
            builder.addAction(Notification.Action.Builder(null, "End", action(ACTION_MIC_OFF, 2)).build())
        }
        return builder.build()
    }

    companion object {
        const val ACTION_START = "dev.rootshell.kai.spike.START"
        const val ACTION_MIC_ON = "dev.rootshell.kai.spike.MIC_ON"
        const val ACTION_MIC_OFF = "dev.rootshell.kai.spike.MIC_OFF"
        const val ACTION_MUTE = "dev.rootshell.kai.spike.MUTE"
        const val ACTION_STOP = "dev.rootshell.kai.spike.STOP"
        const val EXTRA_ORIGIN = "origin"
        private const val NOTIFICATION_ID = 7

        /**
         * specialUse exists from Android 14 (API 34); below it a foreground service needs no type, so
         * the service runs untyped there and only the microphone type is added and dropped.
         */
        @android.annotation.SuppressLint("InlinedApi")
        private val SPECIAL_USE = if (Build.VERSION.SDK_INT >= 34) ServiceInfo.FOREGROUND_SERVICE_TYPE_SPECIAL_USE else 0

        @Volatile var instance: SpikeService? = null
            private set

        private val state = MutableStateFlow(State())
        val states: StateFlow<State> = state

        private fun update(f: (State) -> State) {
            state.value = f(state.value)
        }

        /** Starts the service as specialUse (from a visible activity). */
        fun start(context: Context, origin: String) = send(context, ACTION_START, origin)

        /** Adds the microphone type and starts the capture: on the running service, else by starting it. */
        fun micOn(context: Context, origin: String) {
            val s = instance
            if (s != null) s.setMic(true, origin) else send(context, ACTION_MIC_ON, origin)
        }

        fun micOff(origin: String) {
            instance?.setMic(false, origin)
        }

        fun stop(context: Context, origin: String) {
            if (instance != null) send(context, ACTION_STOP, origin)
        }

        private fun send(context: Context, action: String, origin: String) {
            val intent = Intent(context, SpikeService::class.java).setAction(action).putExtra(EXTRA_ORIGIN, origin)
            val fields = mapOf(
                "action" to action.substringAfterLast('.'), "origin" to origin,
                "activity_visible" to (SpikeApp.visibleActivities > 0), "overlay_showing" to SpikeSession.showing,
                "at_elapsed" to SystemClock.elapsedRealtime(),
            )
            try {
                context.startForegroundService(intent)
                SpikeLog.event("b", "service_start", fields + mapOf("ok" to true))
            } catch (e: Exception) {
                SpikeLog.event("b", "service_start", fields + mapOf("ok" to false, "error" to e.javaClass.name, "message" to e.message))
            }
        }
    }
}
