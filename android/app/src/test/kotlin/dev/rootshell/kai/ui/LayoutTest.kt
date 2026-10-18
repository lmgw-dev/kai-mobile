package dev.rootshell.kai.ui

import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

/** The layouts by size class and font scale (design §10.6), and a scroll's frames (§10.4). */
class LayoutTest {
    @Test
    fun theWidthClassDecidesNotTheDevice() {
        // A foldable's cover screen is narrower than 600 dp, its inner screen wider.
        assertEquals(Layout.Compact, layoutForWidthDp(360))
        assertEquals(Layout.Wide, layoutForWidthDp(600))
        assertEquals(Layout.Wide, layoutForWidthDp(840))
    }

    @Test
    fun theVoicePaneScrollsOnceTheFontNeedsMoreThanTheWindowHas() {
        // A narrow phone screen in portrait (about 360 × 740 dp under the bars), at 100 %.
        assertEquals(VoiceArrangement.TALL, voiceArrangement(740f, 360f, 1f, card = false))
        assertEquals(VoiceArrangement.TALL, voiceArrangement(740f, 360f, 1f, card = true))
        // At 130 % with a card, and at 200 % without one, the column scrolls: the controls stay reachable.
        assertEquals(VoiceArrangement.SCROLL, voiceArrangement(740f, 360f, 1.3f, card = true))
        assertEquals(VoiceArrangement.SCROLL, voiceArrangement(740f, 360f, 2f, card = false))
        // In landscape the ring goes beside the rest.
        assertEquals(VoiceArrangement.SIDE, voiceArrangement(320f, 740f, 1f, card = false))
        // A smaller font never needs less than 100 %'s room.
        assertEquals(VoiceArrangement.TALL, voiceArrangement(440f, 400f, 0.85f, card = false))
    }

    @Test
    fun aScrollsFramesAreCountedAgainstTheirOwnDeadline() {
        val t = FrameTally()
        // 120 Hz: an 8.3 ms deadline; one frame misses it.
        t.add(totalNs = 5_000_000, deadlineNs = 8_333_333)
        t.add(totalNs = 9_000_000, deadlineNs = 8_333_333)
        // The display dropped to 60 Hz: 12 ms is within 16.7 ms.
        t.add(totalNs = 12_000_000, deadlineNs = 16_666_666)
        // No deadline reported: counted, never over.
        t.add(totalNs = 20_000_000, deadlineNs = 0)
        assertEquals(4, t.frames)
        assertEquals(1, t.over)
        assertEquals(20_000_000L, t.worstNs)
        val line = t.summary("conversation", "compact 360 dp")
        assertTrue(line, line.startsWith("conversation: scrolled 4 frames (compact 360 dp), 1 over"))
        assertTrue(line, line.contains("(16.7 ms)") && line.endsWith("the worst 20.0 ms"))
    }
}
