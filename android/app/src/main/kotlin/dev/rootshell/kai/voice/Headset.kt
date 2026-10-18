package dev.rootshell.kai.voice

import android.content.Context
import android.content.Intent
import android.media.AudioAttributes
import android.media.session.MediaSession
import android.media.session.PlaybackState
import android.os.Build
import android.os.Handler
import android.os.Looper
import android.util.Log
import android.view.KeyEvent
import dev.rootshell.kai.core.ffi.HeadsetPress

/**
 * A headset's button as a call reads it (design §8.4), pure so the JVM tests run it: the play/pause
 * and hook keys, a press at its release, a long press (where one arrives: a held key repeating, or
 * flagged long) once, and nothing at the release that follows it. A release Android flags as
 * cancelled is no press. Android's media key handling always keeps the long press of the hook and
 * play/pause keys for voice input (on some phones, e.g. Samsung's, an assistant, which takes the audio focus): the call
 * never sees it, only a play or pause key's.
 */
class HeadsetKeys {
    private var long = false

    fun onKey(keyCode: Int, down: Boolean, repeatCount: Int, longPress: Boolean, cancelled: Boolean): HeadsetPress? {
        if (keyCode !in KEYS) return null
        if (!down && cancelled) {
            long = false
            return null
        }
        return if (down) {
            when {
                repeatCount == 0 && !longPress -> {
                    long = false
                    null
                }
                long -> null
                else -> {
                    long = true
                    HeadsetPress.LONG
                }
            }
        } else if (long) {
            long = false
            null
        } else {
            HeadsetPress.CLICK
        }
    }

    companion object {
        val KEYS = setOf(
            KeyEvent.KEYCODE_HEADSETHOOK,
            KeyEvent.KEYCODE_MEDIA_PLAY_PAUSE,
            KeyEvent.KEYCODE_MEDIA_PLAY,
            KeyEvent.KEYCODE_MEDIA_PAUSE,
        )
    }
}

/**
 * The call's MediaSession (design §8.4): active with a playing `PlaybackState` for the call, so a
 * wired or Bluetooth headset's button reaches Kai instead of the last media app (whose playback
 * would take the focus and end the call); released at the call's end. Its playback is the
 * voice-communication stream, so the volume keys keep adjusting the call. Which presses arrive
 * depends on the phone and headset (WP0 (i)).
 */
class CallButtons(private val context: Context, private val onPress: (HeadsetPress) -> Unit) {
    private val main = Handler(Looper.getMainLooper())
    private var session: MediaSession? = null
    private val keys = HeadsetKeys()

    /** On the main thread; idempotent. */
    fun begin() {
        if (session != null) return
        session = MediaSession(context, "kai-call").apply {
            setCallback(
                object : MediaSession.Callback() {
                    override fun onMediaButtonEvent(intent: Intent): Boolean {
                        val ev = if (Build.VERSION.SDK_INT >= 33) {
                            intent.getParcelableExtra(Intent.EXTRA_KEY_EVENT, KeyEvent::class.java)
                        } else {
                            @Suppress("DEPRECATION") intent.getParcelableExtra(Intent.EXTRA_KEY_EVENT)
                        } ?: return false
                        if (ev.keyCode !in HeadsetKeys.KEYS) return false
                        val press = keys.onKey(ev.keyCode, ev.action == KeyEvent.ACTION_DOWN, ev.repeatCount, ev.isLongPress, ev.isCanceled)
                        if (press != null) {
                            Log.i(TAG, "voice: the headset's ${KeyEvent.keyCodeToString(ev.keyCode)}: $press")
                            onPress(press)
                        }
                        return true
                    }
                },
                main,
            )
            setPlaybackToLocal(
                AudioAttributes.Builder()
                    .setUsage(AudioAttributes.USAGE_VOICE_COMMUNICATION)
                    .setContentType(AudioAttributes.CONTENT_TYPE_SPEECH)
                    .build(),
            )
            setPlaybackState(
                PlaybackState.Builder()
                    .setState(PlaybackState.STATE_PLAYING, PlaybackState.PLAYBACK_POSITION_UNKNOWN, 1f)
                    .setActions(PlaybackState.ACTION_PLAY_PAUSE or PlaybackState.ACTION_PLAY or PlaybackState.ACTION_PAUSE)
                    .build(),
            )
            isActive = true
        }
        Log.i(TAG, "voice: the call's MediaSession is active")
    }

    /** On the main thread. */
    fun end() {
        session?.run {
            isActive = false
            release()
        }
        session = null
    }

    private companion object {
        const val TAG = "kai"
    }
}
