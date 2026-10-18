package dev.rootshell.kai.service

import android.app.Service
import android.content.Context
import android.content.Intent
import android.content.pm.ServiceInfo
import android.os.Build
import android.os.Handler
import android.os.IBinder
import android.os.Looper
import android.util.Log
import dev.rootshell.kai.KaiApplication
import dev.rootshell.kai.R
import dev.rootshell.kai.app.Notifier

/**
 * Kai's service (design M16, §8.6, §12.1): it keeps the process, and with it the held feed, alive for
 * as long as the phone is paired; its one notification shows the link's state, or the call while one
 * runs. `specialUse` from Android 14 (API 34, the type's first level); on 12 and 13 it runs with no
 * type, which needs none there (§8.6's note of 2026-10-15). `START_STICKY`: the system restarts it
 * after the process dies, and the core resumes the feed from its cursor. Started from the visible
 * app once enrolled, at boot and after an update; stopped by Forget gateway.
 *
 * **A call** adds the `microphone` type: [startCall] calls `startForeground` again with
 * `SPECIAL_USE | MICROPHONE` from the visible activity that starts the call (never from the
 * background: the microphone type is while-in-use), and [endCall] with `SPECIAL_USE` alone at the
 * call's end; the service goes on holding the feed. When Android refuses the microphone type (its
 * background-start rules from Android 14: the activity went to the background before the service
 * got there), the call ends with words that say so ([refused]), never running on without the
 * microphone; the service keeps its link's notification.
 */
class KaiService : Service() {
    private var calling = false

    /** The core's number of the call the microphone type is for. */
    private var call: ULong = 0uL

    override fun onCreate() {
        super.onCreate()
        instance = this
        running = true
    }

    override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int {
        if (intent?.action == ACTION_CALL) {
            calling = true
            call = intent.getLongExtra(EXTRA_CALL, 0L).toULong()
        }
        if (foreground()) return START_STICKY
        if (calling) {
            // The microphone type was refused: the call ends saying so; the feed goes on.
            refused(this, call)
            calling = false
            if (foreground()) return START_STICKY
        }
        stopSelf()
        return START_NOT_STICKY
    }

    /** `startForeground` with the notification and the types of now; whether Android took it. */
    private fun foreground(): Boolean {
        val app = application as KaiApplication
        val notification = if (calling) {
            app.notifier.call(app.hub.voice.value, app.core.settings().voice.idleEndS)
        } else {
            app.notifier.status(app.hub.connection.value)
        }
        val types = (if (Build.VERSION.SDK_INT >= 34) ServiceInfo.FOREGROUND_SERVICE_TYPE_SPECIAL_USE else 0) or
            (if (calling) ServiceInfo.FOREGROUND_SERVICE_TYPE_MICROPHONE else 0)
        return try {
            startForeground(Notifier.STATUS_ID, notification, types)
            true
        } catch (e: Exception) {
            val with = if (calling) " with the microphone type" else ""
            Log.w(TAG, "Kai's service could not go to the foreground$with: ${e.javaClass.simpleName}: ${e.message}")
            false
        }
    }

    private fun setCall(on: Boolean, id: ULong) {
        if (on) call = id
        if (calling == on) return
        calling = on
        if (!foreground() && on) {
            // The microphone type was refused: the call ends saying so; the service keeps its
            // link's notification.
            refused(this, id)
            calling = false
            foreground()
        }
    }

    override fun onBind(intent: Intent?): IBinder? = null

    override fun onDestroy() {
        instance = null
        running = false
        // The service's notification goes with it: never posted again at the lock's next change.
        (application as KaiApplication).notifier.gone(null, Notifier.STATUS_ID)
        super.onDestroy()
    }

    companion object {
        private const val TAG = "kai"
        private const val ACTION_CALL = "dev.rootshell.kai.action.SERVICE_CALL"
        private const val EXTRA_CALL = "call"
        private val main = Handler(Looper.getMainLooper())

        @Volatile
        var running = false
            private set

        @Volatile
        private var instance: KaiService? = null

        /** Whether the notification is the call's (the hub refreshes it on a change). */
        val inCall: Boolean
            get() = instance?.calling == true

        /**
         * Starts the service; Android refuses it from the background (logged, never thrown). True
         * when it runs or was started.
         */
        fun start(context: Context): Boolean {
            if (running) return true
            return try {
                context.startForegroundService(Intent(context, KaiService::class.java))
                true
            } catch (e: Exception) {
                Log.w(TAG, "Kai's service was not started: ${e.javaClass.simpleName}: ${e.message}")
                false
            }
        }

        fun stop(context: Context) {
            context.stopService(Intent(context, KaiService::class.java))
        }

        /**
         * Call `call` starts (from a visible activity): the microphone type joins (design §8.6).
         * Refused, the call ends with words that say why.
         */
        fun startCall(context: Context, call: ULong) {
            main.post {
                val s = instance
                if (s != null) {
                    s.setCall(true, call)
                } else {
                    try {
                        context.startForegroundService(
                            Intent(context, KaiService::class.java).setAction(ACTION_CALL).putExtra(EXTRA_CALL, call.toLong()),
                        )
                    } catch (e: Exception) {
                        Log.w(TAG, "Kai's service was not started for the call: ${e.javaClass.simpleName}: ${e.message}")
                        refused(context, call)
                    }
                }
            }
        }

        /** The call ended: the microphone type goes, the service holds the feed on. */
        fun endCall() {
            main.post { instance?.setCall(false, 0uL) }
        }

        /** Android refused the microphone type for call `call`: the call ends in words that say why. */
        private fun refused(context: Context, call: ULong) {
            Log.w(TAG, "voice: Android refused the microphone type; call $call ends")
            val app = context.applicationContext as KaiApplication
            app.core.voice.failed(call, app.getString(R.string.voice_microphone_refused))
        }
    }
}
