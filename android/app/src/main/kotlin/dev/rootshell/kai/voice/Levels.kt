package dev.rootshell.kai.voice

import kotlin.math.log10
import kotlin.math.sqrt

/**
 * The ring's levels (design §10.3): the microphone's from the device module's captured samples and
 * Kai's from its played samples (the libwebrtc build's playback callback: what the speaker plays,
 * so a stop's local mute reads as silence at once), each the RMS of one 10 ms buffer as libwebrtc
 * hands it over. Written on libwebrtc's audio threads, read by the ring at its frame rate; no
 * polling, no getStats.
 */
class Levels {
    @Volatile var mic: Float = 0f

    @Volatile var kai: Float = 0f

    fun reset() {
        mic = 0f
        kai = 0f
    }
}

object Level {
    /** Below this the level is 0 (a quiet room's floor). */
    const val FLOOR_DB = -60.0

    /**
     * The RMS of 16-bit little-endian PCM as a level from 0 ([FLOOR_DB] and below) to 1 (full
     * scale), linear in dB.
     */
    fun of(pcm: ByteArray): Float {
        val n = pcm.size / 2
        if (n == 0) return 0f
        var sum = 0.0
        for (i in 0 until n) {
            val lo = pcm[2 * i].toInt() and 0xff
            val hi = pcm[2 * i + 1].toInt()
            val s = ((hi shl 8) or lo).toShort() / 32768.0
            sum += s * s
        }
        val rms = sqrt(sum / n)
        if (rms <= 0.0) return 0f
        val db = 20 * log10(rms)
        return ((db - FLOOR_DB) / -FLOOR_DB).coerceIn(0.0, 1.0).toFloat()
    }
}
