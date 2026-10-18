package dev.rootshell.kai.voice

import android.os.Handler
import android.os.Looper
import android.util.Log
import dev.rootshell.kai.KaiApplication
import dev.rootshell.kai.R
import dev.rootshell.kai.core.ffi.AudioFocus
import dev.rootshell.kai.core.ffi.CallEnd
import dev.rootshell.kai.core.ffi.EndReason
import dev.rootshell.kai.service.KaiService

/**
 * The controller's decisions around a call's audio (design §8.5, §9.3, §10.5), pure so the JVM tests
 * run them; [VoiceController] carries them out on the main thread.
 * - **A call's start** sets the audio up once the focus is granted; a refused focus sets nothing up
 *   (no session, no route, no earcon, no haptic) and the call fails. A call that starts while the
 *   audio is up keeps it: a rebind's next call, or one started while the last end's earcon plays,
 *   whose release is called off.
 * - **Another app holding the audio** (a transient loss until the focus is back, or a loss for good)
 *   silences Kai's own sounds: no cue, no ending earcon and no haptic play into its audio. An end as
 *   "audio taken" is the same.
 * - **The end** plays its earcon on the call's route, and the audio goes once that earcon played
 *   (each end's earcon has its own number, so an older one's report releases nothing).
 */
class CallLife {
    /** The core's number of the call the audio is for (focus changes name it). */
    var call: ULong = 0uL
        private set

    /** The call's audio is up: the focus granted, the microphone type, the session, the route. */
    var active = false
        private set

    /** Another app holds the audio (from a loss until the focus is back). */
    var away = false
        private set

    /** The ending earcon whose end releases the audio (0: none pending). */
    private var releasing = 0
    private var endings = 0

    enum class Start {
        /** Ask for the focus, then set the audio up. */
        SET_UP,

        /** The audio is up already and stays. */
        KEEP,
    }

    /** What a call's end does: its earcon, the reject haptic, and the number its release waits for. */
    data class Ending(val earcon: Boolean, val reject: Boolean, val token: Int)

    fun starting(id: ULong): Start {
        call = id
        if (!active) return Start.SET_UP
        // A pending release (the last end's earcon) is called off: the new call keeps the audio.
        releasing = 0
        return Start.KEEP
    }

    /** The focus request's answer for a [Start.SET_UP] start: whether the audio is set up. */
    fun focus(granted: Boolean): Boolean {
        if (granted) {
            active = true
            away = false
        }
        return granted
    }

    /** Android's focus change, as the call reads it ([Focus.of]). */
    fun focusChanged(focus: AudioFocus) {
        away = focus != AudioFocus.GAINED
    }

    /** The cue's earcon and tick, as the settings allow: none while another app holds the audio. */
    fun cue(earcon: Boolean, haptics: Boolean): Pair<Boolean, Boolean> {
        val heard = active && !away
        return (heard && earcon) to (heard && haptics)
    }

    /**
     * Call [call] ended for the owner (not a rebind). Null when its audio was never set up: nothing
     * to release, nothing to play.
     */
    fun ended(reason: EndReason, earcon: Boolean, haptics: Boolean): Ending? {
        if (!active) return null
        val quiet = away || reason == EndReason.AUDIO_TAKEN
        endings += 1
        releasing = endings
        val failed = reason == EndReason.REFUSED || reason == EndReason.FAILED
        return Ending(earcon = earcon && !quiet, reject = haptics && failed && !quiet, token = endings)
    }

    /** End `token`'s earcon played (or none played): true when the audio is to be released now. */
    fun played(token: Int): Boolean {
        if (token == 0 || token != releasing) return false
        releasing = 0
        active = false
        away = false
        return true
    }
}

/**
 * The app's voice controller (design §9.3): the call's audio around the libwebrtc side, for the
 * process's life, so an activity a fold recreates finds the call, its route and its session where
 * they were. On the main thread, in the order the engine reports, by [CallLife]'s decisions:
 * - **a call's start** (§7.3 step 2): the audio focus first (§8.5; refused, the call ends in words
 *   that say so and nothing else is set up), then the microphone type on Kai's service, the
 *   MediaSession for the headset's buttons (§8.4), `MODE_IN_COMMUNICATION` and the route (§8.3). A
 *   rebind's next call keeps all of it;
 * - **the cue** (§10.5): the listening earcon and the confirm tick;
 * - **the ask** (§10.5, §11): a call waits for the owner's verdict in the call, its earcon and haptic;
 * - **the end**: the reject haptic for a call that could not start or go on, the ending earcon on
 *   the call's route, then (once it played) the focus abandoned, the session released, the route
 *   cleared and the microphone type dropped. Neither plays while another app holds the audio.
 */
class VoiceController(private val app: KaiApplication) : CallEngine.Listener {
    val audio = CallAudio(app)
    private val buttons = CallButtons(app) { press -> app.core.voice.headset(press) }
    private val earcons = Earcons(app)
    private val main = Handler(Looper.getMainLooper())
    private val life = CallLife()

    override fun callStarting(call: ULong) {
        main.post {
            if (life.starting(call) == CallLife.Start.KEEP) {
                KaiService.startCall(app, call)
                // A rebind's next call during a pause starts paused too: the focus is still away.
                if (life.away) app.core.voice.focus(call, AudioFocus.LOST_TRANSIENT)
                return@post
            }
            val focused = audio.requestFocus { change ->
                Focus.of(change)?.let { focus ->
                    Log.i(TAG, "voice: the audio focus changed ($change): $focus")
                    life.focusChanged(focus)
                    app.core.voice.focus(life.call, focus)
                }
            }
            if (!life.focus(focused)) {
                app.core.voice.failed(call, app.getString(R.string.voice_focus_refused))
                return@post
            }
            val settings = app.core.settings()
            KaiService.startCall(app, call)
            buttons.begin()
            audio.begin(settings.voice.route, settings.lmgw.timeoutS)
        }
    }

    override fun callEnded(end: CallEnd) {
        main.post {
            val settings = app.core.settings()
            val ending = life.ended(end.reason, settings.voice.earcon, settings.voice.haptics) ?: return@post
            if (ending.reject) earcons.reject()
            // The ending earcon plays on the call's route before the route goes.
            val playing = ending.earcon && earcons.play(Earcon.ENDED, bound(settings.lmgw.timeoutS)) { played(ending.token) }
            if (!playing) played(ending.token)
        }
    }

    override fun cue() {
        main.post {
            val settings = app.core.settings()
            val (sound, tick) = life.cue(settings.voice.earcon, settings.voice.haptics)
            if (sound) earcons.play(Earcon.LISTENING, bound(settings.lmgw.timeoutS))
            if (tick) earcons.tick()
        }
    }

    /** The ask (§10.5, §11): by the cue's rules, so nothing plays into another app's audio. */
    override fun asked() {
        main.post {
            val settings = app.core.settings()
            val (sound, buzz) = life.cue(settings.voice.earcon, settings.voice.haptics)
            if (sound) earcons.play(Earcon.ASK, bound(settings.lmgw.timeoutS))
            if (buzz) earcons.ask()
        }
    }

    /** The route button's choice for the rest of the call. */
    fun choose(id: Int) = main.post { audio.choose(id) }

    private fun played(token: Int) {
        if (!life.played(token)) return
        audio.abandonFocus()
        buttons.end()
        audio.end()
        KaiService.endCall()
    }

    private fun bound(timeoutS: UInt): Long = timeoutS.toLong() * 1000

    private companion object {
        const val TAG = "kai"
    }
}
