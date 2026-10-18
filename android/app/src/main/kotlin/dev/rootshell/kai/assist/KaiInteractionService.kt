package dev.rootshell.kai.assist

import android.app.ActivityManager
import android.content.Intent
import android.os.Bundle
import android.os.Process
import android.os.SystemClock
import android.service.voice.VoiceInteractionService
import android.service.voice.VoiceInteractionSession
import android.service.voice.VoiceInteractionSessionService
import android.speech.RecognitionService
import android.speech.SpeechRecognizer
import android.util.Log
import dev.rootshell.kai.KaiApplication

/**
 * Kai as the system's digital assistant (design §9.1, §9.2, M13): the side button's press and hold
 * on Samsung phones, the power button's long press where the system gives it to the assistant, the
 * corner swipe, a headset's assistant button (§9.4) all arrive as a show of [KaiSession].
 *
 * The system binds this service while Kai holds the assistant role, which starts the process (and
 * with it the core, its pairing and Kai's service, M16). `onReady` prepares libwebrtc's factory and
 * its device module, no network and no microphone (§7.2), so an invocation only builds the offer.
 */
class KaiInteractionService : VoiceInteractionService() {
    override fun onReady() {
        super.onReady()
        // No screen content is collected in v1 (§9.2, §9.7, Q8): the system neither reads the
        // screen for Kai nor spends the time on it.
        disabledShowContext = NO_SCREEN_CONTEXT
        (application as KaiApplication).calls.prepareFactory()
        Log.i(TAG, "assist: ready (the process is ${processAgeMs()} ms old, importance ${importance()})")
    }

    override fun onShutdown() {
        Log.i(TAG, "assist: shut down (the process is ${processAgeMs()} ms old)")
        super.onShutdown()
    }

    override fun onLaunchVoiceAssistFromKeyguard() {
        // supportsLaunchVoiceAssistFromKeyguard is false (§9.6): logged if it comes anyway, and
        // nothing starts.
        Log.w(TAG, "assist: asked to launch from the lock screen, which Kai does not offer (design §9.6)")
    }

    companion object {
        private const val TAG = "kai"

        /**
         * **WP0 (a)'s seam:** what the system is asked not to collect for Kai's sessions
         * (`setDisabledShowContext`). WP0 (a) confirms that it takes effect (no
         * `onHandleAssist`/`onHandleScreenshot` with it set); [KaiSession] logs any that comes and
         * reads nothing of it.
         */
        const val NO_SCREEN_CONTEXT =
            VoiceInteractionSession.SHOW_WITH_ASSIST or VoiceInteractionSession.SHOW_WITH_SCREENSHOT

        /** The process's age: a show in a young process is a cold one (§10.4). */
        fun processAgeMs(): Long = SystemClock.elapsedRealtime() - Process.getStartElapsedRealtime()

        /** The process's importance as the system rates it now (§9.2: recorded, not relied on). */
        fun importance(): Int = ActivityManager.RunningAppProcessInfo().also {
            ActivityManager.getMyMemoryState(it)
        }.importance
    }
}

/** Makes the overlay's session (design §9.1). */
class KaiSessionService : VoiceInteractionSessionService() {
    override fun onNewSession(args: Bundle?): VoiceInteractionSession = KaiSession(this)
}

/**
 * The stub recogniser AOSP requires beside an assistant (design §9.1): every request is answered
 * with an error at once. While Kai is the assistant, apps that use the system's default speech
 * recogniser get none from it (setup says so); keyboards with a recogniser of their own are not
 * affected. A recogniser on lmgw's ASR is Later.
 */
class KaiRecognitionService : RecognitionService() {
    override fun onStartListening(recognizerIntent: Intent?, listener: Callback?) {
        Log.i(TAG, "assist: a speech recognition request (uid ${listener?.callingUid}) is answered with an error: Kai offers no recogniser (design §9.1)")
        runCatching { listener?.error(SpeechRecognizer.ERROR_CLIENT) }
    }

    override fun onCancel(listener: Callback?) {}

    override fun onStopListening(listener: Callback?) {}

    private companion object {
        const val TAG = "kai"
    }
}
