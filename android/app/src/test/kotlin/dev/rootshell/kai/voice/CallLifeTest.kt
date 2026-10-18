package dev.rootshell.kai.voice

import dev.rootshell.kai.core.ffi.AudioFocus
import dev.rootshell.kai.core.ffi.EndReason
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

/** The controller's decisions around a call's audio (design §8.5, §9.3, §10.5). */
class CallLifeTest {
    private fun up(id: ULong = 1uL): CallLife = CallLife().also {
        assertEquals(CallLife.Start.SET_UP, it.starting(id))
        assertTrue(it.focus(granted = true))
    }

    @Test
    fun focusRefusedAtStartBeginsNothing() {
        val life = CallLife()
        assertEquals(CallLife.Start.SET_UP, life.starting(1uL))
        assertFalse(life.focus(granted = false))
        assertFalse(life.active)
        // The core's end for it: nothing to release, no earcon, no haptic.
        assertNull(life.ended(EndReason.FAILED, earcon = true, haptics = true))
        assertEquals(false to false, life.cue(earcon = true, haptics = true))
        // The next call asks again.
        assertEquals(CallLife.Start.SET_UP, life.starting(2uL))
    }

    @Test
    fun theEndPlaysThenReleasesOnceItsEarconPlayed() {
        val life = up()
        assertEquals(true to true, life.cue(earcon = true, haptics = true))
        val ending = life.ended(EndReason.FAILED, earcon = true, haptics = true)!!
        assertTrue(ending.earcon)
        assertTrue(ending.reject)
        assertTrue(life.active)
        assertTrue(life.played(ending.token))
        assertFalse(life.active)
        // Reported twice: released once.
        assertFalse(life.played(ending.token))
        // The settings off: no sound, no haptic, the release at once.
        val quiet = up().ended(EndReason.REFUSED, earcon = false, haptics = false)!!
        assertFalse(quiet.earcon || quiet.reject)
        // The owner's End: the earcon, no reject haptic.
        assertFalse(up().ended(EndReason.ENDED, earcon = true, haptics = true)!!.reject)
    }

    @Test
    fun aNewCallStartsWhileTheEndsEarconPlays() {
        val life = up()
        val first = life.ended(EndReason.ENDED, earcon = true, haptics = true)!!
        // Talk again before the falling notes are over: the audio stays up.
        assertEquals(CallLife.Start.KEEP, life.starting(2uL))
        assertEquals(2uL, life.call)
        assertFalse("the old earcon releases nothing", life.played(first.token))
        assertTrue(life.active)
        val second = life.ended(EndReason.ENDED, earcon = true, haptics = true)!!
        assertTrue(life.played(second.token))
        assertFalse(life.active)
    }

    @Test
    fun anOlderEndsEarconDoesNotCutTheNextEndsShort() {
        val life = up()
        val first = life.ended(EndReason.ENDED, earcon = true, haptics = true)!!
        life.starting(2uL)
        val second = life.ended(EndReason.ENDED, earcon = true, haptics = true)!!
        assertFalse(life.played(first.token))
        assertTrue(life.active)
        assertTrue(life.played(second.token))
    }

    @Test
    fun aRebindsNextCallKeepsTheAudioAndItsPause() {
        val life = up()
        life.focusChanged(AudioFocus.LOST_TRANSIENT)
        assertEquals(CallLife.Start.KEEP, life.starting(2uL))
        assertTrue(life.active)
        assertTrue("the next call starts paused", life.away)
    }

    @Test
    fun nothingOfKaisPlaysIntoAnotherAppsAudio() {
        val life = up()
        life.focusChanged(AudioFocus.LOST_TRANSIENT)
        assertEquals(false to false, life.cue(earcon = true, haptics = true))
        val paused = life.ended(EndReason.FAILED, earcon = true, haptics = true)!!
        assertFalse(paused.earcon)
        assertFalse(paused.reject)
        assertTrue(life.played(paused.token))
        // The focus back: the sounds again.
        val back = up()
        back.focusChanged(AudioFocus.LOST_TRANSIENT)
        back.focusChanged(AudioFocus.GAINED)
        assertEquals(true to true, back.cue(earcon = true, haptics = true))
        assertTrue(back.ended(EndReason.IDLE, earcon = true, haptics = true)!!.earcon)
        // Audio taken (a loss for good) ends silent.
        val taken = up()
        taken.focusChanged(AudioFocus.LOST)
        val end = taken.ended(EndReason.AUDIO_TAKEN, earcon = true, haptics = true)!!
        assertFalse(end.earcon || end.reject)
        assertFalse(up().ended(EndReason.AUDIO_TAKEN, earcon = true, haptics = true)!!.earcon)
    }
}
