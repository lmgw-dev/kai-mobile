package dev.rootshell.kai.voice

import kotlin.math.PI
import kotlin.math.abs
import kotlin.math.roundToInt
import kotlin.math.sin
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

/** The earcons' sound and the ring's level, pure. */
class SoundTest {
    @Test
    fun anEarconIsTwoNotesWithAGapAndNoClick() {
        for (kind in Earcon.entries) {
            val pcm = Tones.samples(kind)
            val note = Tones.RATE * Tones.NOTE_MS / 1000
            val gap = Tones.RATE * Tones.GAP_MS / 1000
            assertEquals(kind.name, 2 * note + gap, pcm.size)
            assertEquals(kind.name, 190L, Tones.durationMs(kind))
            // Silent at both ends of each note (the fades) and in the gap.
            assertEquals(kind.name, 0, pcm.first().toInt())
            assertTrue(kind.name, abs(pcm[note - 1].toInt()) < 200)
            assertTrue(kind.name, (note until note + gap).all { pcm[it].toInt() == 0 })
            assertTrue(kind.name, abs(pcm.last().toInt()) < 200)
            // Never above its peak.
            val peak = (Tones.PEAK * Short.MAX_VALUE).roundToInt()
            assertTrue(kind.name, pcm.all { abs(it.toInt()) <= peak })
            assertTrue(kind.name, pcm.any { abs(it.toInt()) > peak * 9 / 10 })
        }
    }

    @Test
    fun listeningRisesTheEndFallsAndTheAskRepeats() {
        val (a, b) = Tones.notes(Earcon.LISTENING)
        assertTrue(b > a)
        val (c, d) = Tones.notes(Earcon.ENDED)
        assertTrue(d < c)
        // The ask is neither: one note twice, above both.
        val (e, f) = Tones.notes(Earcon.ASK)
        assertEquals(e, f, 0.0)
        assertTrue(e > b && e > c)
    }

    private fun pcm(amplitude: Double, n: Int = 480): ByteArray {
        val out = ByteArray(n * 2)
        for (i in 0 until n) {
            val s = (amplitude * 32767 * sin(2 * PI * 440 * i / 48_000)).roundToInt()
            out[2 * i] = (s and 0xff).toByte()
            out[2 * i + 1] = (s shr 8).toByte()
        }
        return out
    }

    @Test
    fun theLevelIsLinearInDecibelsAboveTheFloor() {
        assertEquals(0f, Level.of(ByteArray(960)), 0f)
        assertEquals(0f, Level.of(ByteArray(0)), 0f)
        // A full-scale sine is −3 dBFS RMS: 57/60.
        assertEquals(57f / 60f, Level.of(pcm(1.0)), 0.01f)
        // −40 dBFS peak, −43 dBFS RMS: 17/60.
        assertEquals(17f / 60f, Level.of(pcm(0.01)), 0.02f)
        // Below the floor: 0.
        assertEquals(0f, Level.of(pcm(0.0005)), 0f)
    }
}
