package dev.rootshell.kai.voice

import android.media.AudioManager
import android.view.KeyEvent
import dev.rootshell.kai.core.ffi.AudioFocus
import dev.rootshell.kai.core.ffi.EchoMode
import dev.rootshell.kai.core.ffi.HeadsetPress
import dev.rootshell.kai.core.ffi.RingState
import dev.rootshell.kai.ui.voice.RingMotion
import dev.rootshell.kai.ui.voice.ringMotion
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

/** The call's pure decisions on the phone's side: focus, headset keys, the echo canceller, the ring's motion. */
class CallDecisionsTest {
    @Test
    fun focusChangesAsTheCallReadsThem() {
        assertEquals(AudioFocus.LOST, Focus.of(AudioManager.AUDIOFOCUS_LOSS))
        assertEquals(AudioFocus.LOST_TRANSIENT, Focus.of(AudioManager.AUDIOFOCUS_LOSS_TRANSIENT))
        assertEquals(AudioFocus.GAINED, Focus.of(AudioManager.AUDIOFOCUS_GAIN))
        assertEquals(AudioFocus.GAINED, Focus.of(AudioManager.AUDIOFOCUS_GAIN_TRANSIENT))
        // A loss that only asks to duck is no loss for a call.
        assertNull(Focus.of(AudioManager.AUDIOFOCUS_LOSS_TRANSIENT_CAN_DUCK))
        assertNull(Focus.of(AudioManager.AUDIOFOCUS_NONE))
    }

    private fun HeadsetKeys.press(key: Int = KeyEvent.KEYCODE_HEADSETHOOK): List<HeadsetPress?> =
        listOf(
            onKey(key, down = true, repeatCount = 0, longPress = false, cancelled = false),
            onKey(key, down = false, repeatCount = 0, longPress = false, cancelled = false),
        )

    @Test
    fun aHeadsetPressCountsAtItsRelease() {
        val keys = HeadsetKeys()
        assertEquals(listOf(null, HeadsetPress.CLICK), keys.press())
        assertEquals(listOf(null, HeadsetPress.CLICK), keys.press(KeyEvent.KEYCODE_MEDIA_PLAY_PAUSE))
        // A Bluetooth headset may send play or pause, by the state the session reports.
        assertEquals(listOf(null, HeadsetPress.CLICK), keys.press(KeyEvent.KEYCODE_MEDIA_PAUSE))
        assertEquals(listOf(null, HeadsetPress.CLICK), keys.press(KeyEvent.KEYCODE_MEDIA_PLAY))
    }

    @Test
    fun aLongPressCountsOnceAndItsReleaseNotAtAll() {
        val keys = HeadsetKeys()
        val k = KeyEvent.KEYCODE_HEADSETHOOK
        assertNull(keys.onKey(k, down = true, repeatCount = 0, longPress = false, cancelled = false))
        assertEquals(HeadsetPress.LONG, keys.onKey(k, down = true, repeatCount = 1, longPress = true, cancelled = false))
        assertNull(keys.onKey(k, down = true, repeatCount = 2, longPress = false, cancelled = false))
        assertNull(keys.onKey(k, down = false, repeatCount = 0, longPress = false, cancelled = false))
        // The next press is a press again.
        assertEquals(listOf(null, HeadsetPress.CLICK), keys.press())
    }

    @Test
    fun aCancelledReleaseIsNoPress() {
        val keys = HeadsetKeys()
        val k = KeyEvent.KEYCODE_MEDIA_PLAY_PAUSE
        assertNull(keys.onKey(k, down = true, repeatCount = 0, longPress = false, cancelled = false))
        assertNull(keys.onKey(k, down = false, repeatCount = 0, longPress = false, cancelled = true))
        // A long press whose release is cancelled leaves the next press a press.
        assertEquals(HeadsetPress.LONG, keys.onKey(k, down = true, repeatCount = 1, longPress = true, cancelled = false))
        assertNull(keys.onKey(k, down = false, repeatCount = 0, longPress = false, cancelled = true))
        assertEquals(listOf(null, HeadsetPress.CLICK), keys.press(k))
    }

    @Test
    fun otherKeysAreNotTheCalls() {
        val keys = HeadsetKeys()
        assertNull(keys.onKey(KeyEvent.KEYCODE_MEDIA_NEXT, down = false, repeatCount = 0, longPress = false, cancelled = false))
        assertNull(keys.onKey(KeyEvent.KEYCODE_VOLUME_UP, down = false, repeatCount = 0, longPress = false, cancelled = false))
    }

    @Test
    fun theHardwareCancellerByTheEchoSetting() {
        assertTrue(CallEngine.hardwareCanceller(EchoMode.HARDWARE, available = false))
        assertFalse(CallEngine.hardwareCanceller(EchoMode.SOFTWARE, available = true))
        // Half duplex runs as software on the phone (§8.2).
        assertFalse(CallEngine.hardwareCanceller(EchoMode.HALF_DUPLEX, available = true))
        assertTrue(CallEngine.hardwareCanceller(EchoMode.AUTO, available = true))
        assertFalse(CallEngine.hardwareCanceller(EchoMode.AUTO, available = false))
    }

    @Test
    fun theRingMovesOnlyWhileTheCallIsActive() {
        assertEquals(RingMotion.MIC, ringMotion(RingState.LISTENING))
        assertEquals(RingMotion.MIC, ringMotion(RingState.INTERRUPTED))
        assertEquals(RingMotion.KAI, ringMotion(RingState.SPEAKING))
        assertEquals(RingMotion.SPIN, ringMotion(RingState.THINKING))
        assertEquals(RingMotion.BREATHE, ringMotion(RingState.CONNECTING))
        for (still in listOf(RingState.REST, RingState.IDLE, RingState.MUTED, RingState.PENDING)) {
            assertEquals(still.name, RingMotion.STATIC, ringMotion(still))
        }
    }
}
