package dev.rootshell.kai.spike.assist

import android.app.ActivityManager
import android.content.Intent
import android.os.Bundle
import android.os.SystemClock
import android.service.voice.VoiceInteractionService
import android.service.voice.VoiceInteractionSession
import android.service.voice.VoiceInteractionSessionService
import android.speech.RecognitionService
import android.speech.SpeechRecognizer
import dev.rootshell.kai.spike.Prefs
import dev.rootshell.kai.spike.SpikeApp
import dev.rootshell.kai.spike.SpikeLog
import dev.rootshell.kai.spike.webrtc.WebRtc

/**
 * The assistant (design §9.1, §9.2; WP0 (a)). `onReady` prepares libwebrtc's factory and device
 * module, as Kai will, and applies `setDisabledShowContext` as (a)'s toggle has it. The app can
 * show the overlay itself through [showFromApp] while the spike is the active assistant (the
 * service instance lives in this process then).
 */
class SpikeInteractionService : VoiceInteractionService() {
    override fun onReady() {
        super.onReady()
        instance = this
        val disable = Prefs.disableShowContext(this)
        disabledShowContext = if (disable) {
            VoiceInteractionSession.SHOW_WITH_ASSIST or VoiceInteractionSession.SHOW_WITH_SCREENSHOT
        } else {
            0
        }
        val age = SpikeApp.processAgeMs()
        val prep = WebRtc.prepare(applicationContext)
        readyElapsed = SystemClock.elapsedRealtime()
        SpikeLog.event(
            "a", "vis_ready",
            mapOf(
                "process_age_ms" to age, "webrtc" to prep.toMap(),
                "disabled_show_context" to disabledShowContext, "importance" to importance(),
            ),
        )
    }

    override fun onShutdown() {
        SpikeLog.event("a", "vis_shutdown", mapOf("process_age_ms" to SpikeApp.processAgeMs()))
        instance = null
        super.onShutdown()
    }

    override fun onLaunchVoiceAssistFromKeyguard() {
        // supportsLaunchVoiceAssistFromKeyguard is false (design §9.6); logged if it comes anyway.
        SpikeLog.event("a", "keyguard_launch")
    }

    /** Shows the overlay as the app (a, n, o), with the time of the request. */
    fun showFromApp(source: String) {
        showSession(
            Bundle().apply {
                putString(EXTRA_SOURCE, source)
                putLong(EXTRA_T0, SystemClock.elapsedRealtime())
            },
            0,
        )
    }

    companion object {
        const val EXTRA_SOURCE = "spike_source"
        const val EXTRA_T0 = "spike_t0_elapsed"

        @Volatile var instance: SpikeInteractionService? = null
            private set

        /** elapsedRealtime of the last onReady in this process, 0 if none. */
        @Volatile var readyElapsed = 0L
            private set

        fun importance(): Int = ActivityManager.RunningAppProcessInfo().also {
            ActivityManager.getMyMemoryState(it)
        }.importance
    }
}

/** Creates the overlay's session (design §9.1). */
class SpikeSessionService : VoiceInteractionSessionService() {
    override fun onNewSession(args: Bundle?): VoiceInteractionSession {
        SpikeSession.newSessionNanos = System.nanoTime()
        SpikeSession.sessionsInProcess++
        return SpikeSession(this)
    }
}

/**
 * The stub recogniser AOSP requires beside the assistant (design §9.1): every request is answered
 * with an error at once. Each request is logged, so (a) shows which apps asked for recognition.
 */
class SpikeRecognitionService : RecognitionService() {
    override fun onStartListening(recognizerIntent: Intent?, listener: Callback?) {
        SpikeLog.event("a", "recognizer_request", mapOf("caller_uid" to listener?.callingUid))
        runCatching { listener?.error(SpeechRecognizer.ERROR_CLIENT) }
    }

    override fun onCancel(listener: Callback?) {}

    override fun onStopListening(listener: Callback?) {}
}
