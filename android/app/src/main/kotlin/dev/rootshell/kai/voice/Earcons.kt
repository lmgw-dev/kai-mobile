package dev.rootshell.kai.voice

import android.content.Context
import android.media.AudioFormat
import android.media.AudioTrack
import android.os.Handler
import android.os.Looper
import android.os.VibrationEffect
import android.os.VibratorManager
import android.util.Log
import kotlin.math.PI
import kotlin.math.cos
import kotlin.math.roundToInt
import kotlin.math.sin

/**
 * The call's earcons (design §10.5): listening starts, the call ends, and a call waits for the
 * owner's verdict in the call (§11), so an owner listening with the screen off knows to look.
 */
enum class Earcon { LISTENING, ENDED, ASK }

/**
 * The earcons' sound, pure so the JVM tests check it: two short sine notes, rising when listening
 * starts, falling when a call ends and the same note twice when a verdict is asked for, each with a
 * raised-cosine fade in and out (no click) and a short gap between, at 48 kHz mono, peaking at
 * [PEAK] of full scale.
 */
object Tones {
    const val RATE = 48_000
    const val NOTE_MS = 80
    const val GAP_MS = 30
    const val FADE_MS = 12
    const val PEAK = 0.35

    /** The notes in Hz: E5 to A5 rising, A5 to D5 falling, B5 twice for the ask. */
    fun notes(kind: Earcon): List<Double> = when (kind) {
        Earcon.LISTENING -> listOf(659.25, 880.0)
        Earcon.ENDED -> listOf(880.0, 587.33)
        Earcon.ASK -> listOf(987.77, 987.77)
    }

    fun samples(kind: Earcon): ShortArray {
        val note = RATE * NOTE_MS / 1000
        val gap = RATE * GAP_MS / 1000
        val fade = RATE * FADE_MS / 1000
        val notes = notes(kind)
        val out = ShortArray(notes.size * note + (notes.size - 1) * gap)
        notes.forEachIndexed { n, hz ->
            val start = n * (note + gap)
            for (i in 0 until note) {
                val envelope = when {
                    i < fade -> 0.5 - 0.5 * cos(PI * i / fade)
                    i >= note - fade -> 0.5 - 0.5 * cos(PI * (note - 1 - i) / fade)
                    else -> 1.0
                }
                val v = PEAK * envelope * sin(2 * PI * hz * i / RATE)
                out[start + i] = (v * Short.MAX_VALUE).roundToInt().toShort()
            }
        }
        return out
    }

    /** How long `kind` plays, in milliseconds. */
    fun durationMs(kind: Earcon): Long = samples(kind).size * 1000L / RATE
}

/**
 * Plays the earcons on the communication stream (so they go where the call's audio goes, a headset
 * included, at the call's volume) and the haptic ticks (design §10.5), as `voice.earcon` and
 * `voice.haptics` allow; the caller checks the settings.
 */
class Earcons(private val context: Context) {
    private val main = Handler(Looper.getMainLooper())

    /**
     * Plays `kind`; `onPlayed` (on the main thread) once the track's playback head has passed its
     * last frame, which on a Bluetooth headset's SCO link comes later than the sound's nominal
     * length. A track that never reports it (a dead route) is given up after its length and
     * `boundMs` (`lmgw.timeout_s`), logged. False, and no `onPlayed`, when it could not play.
     */
    fun play(kind: Earcon, boundMs: Long, onPlayed: (() -> Unit)? = null): Boolean {
        val pcm = Tones.samples(kind)
        var track: AudioTrack? = null
        return try {
            val t = AudioTrack.Builder()
                .setAudioAttributes(CallAudio.ATTRIBUTES)
                .setAudioFormat(
                    AudioFormat.Builder()
                        .setSampleRate(Tones.RATE)
                        .setChannelMask(AudioFormat.CHANNEL_OUT_MONO)
                        .setEncoding(AudioFormat.ENCODING_PCM_16BIT)
                        .build(),
                )
                .setTransferMode(AudioTrack.MODE_STATIC)
                .setBufferSizeInBytes(pcm.size * 2)
                .build()
            track = t
            // A static track holds a native player: released once it played out (or given up).
            var done = false
            lateinit var bound: Runnable
            val finish = { why: String? ->
                if (!done) {
                    done = true
                    main.removeCallbacks(bound)
                    why?.let { Log.w(TAG, "voice: the $kind earcon $it; its track is released") }
                    t.release()
                    onPlayed?.invoke()
                }
            }
            bound = Runnable {
                finish("did not report its end within its length and lmgw.timeout_s (${boundMs / 1000} s)")
            }
            t.setPlaybackPositionUpdateListener(
                object : AudioTrack.OnPlaybackPositionUpdateListener {
                    override fun onMarkerReached(track: AudioTrack) = finish(null)
                    override fun onPeriodicNotification(track: AudioTrack) {}
                },
                main,
            )
            // Mono: one frame per sample. The marker is the last frame played.
            check(t.setNotificationMarkerPosition(pcm.size) == AudioTrack.SUCCESS) { "the end marker was refused" }
            val written = t.write(pcm, 0, pcm.size)
            check(written == pcm.size) { "write returned $written of ${pcm.size}" }
            t.play()
            main.postDelayed(bound, Tones.durationMs(kind) + boundMs)
            true
        } catch (e: Exception) {
            Log.w(TAG, "voice: the earcon did not play: ${e.javaClass.simpleName}: ${e.message}")
            track?.release()
            false
        }
    }

    /** The confirm tick when listening starts. */
    fun tick() = vibrate(VibrationEffect.EFFECT_TICK)

    /** The reject haptic when a call could not start or the phone could not carry it on. */
    fun reject() = vibrate(VibrationEffect.EFFECT_DOUBLE_CLICK)

    /** The ask's haptic: a call waits for the owner's verdict (§11). */
    fun ask() = vibrate(VibrationEffect.EFFECT_HEAVY_CLICK)

    private fun vibrate(effect: Int) {
        runCatching {
            context.getSystemService(VibratorManager::class.java)?.defaultVibrator
                ?.vibrate(VibrationEffect.createPredefined(effect))
        }.onFailure { Log.w(TAG, "voice: the haptic failed: ${it.message}") }
    }

    private companion object {
        const val TAG = "kai"
    }
}
