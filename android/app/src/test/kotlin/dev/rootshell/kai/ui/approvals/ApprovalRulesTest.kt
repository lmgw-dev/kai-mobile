package dev.rootshell.kai.ui.approvals

import dev.rootshell.kai.core.ffi.ApprovalView
import dev.rootshell.kai.core.ffi.CallPhase
import dev.rootshell.kai.core.ffi.RingState
import dev.rootshell.kai.core.ffi.VoiceView
import dev.rootshell.kai.ui.Layout
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

private fun card(
    id: String,
    message: Long = 30,
    verdict: Boolean? = null,
    sent: Boolean = false,
    inCall: Boolean = false,
) = ApprovalView(
    approvalRequestId = id, threadId = 7, threadTitle = "Kai", messageId = message, serverLabel = "desktop-a",
    name = "open_file", arguments = "path: /tmp/x", verdict = verdict, sent = sent, waiting = 0u, inCall = inCall,
    note = null,
)

private fun call(phase: CallPhase, thread: Long, awaiting: List<String>) = VoiceView(
    call = 1u, phase = phase, ring = RingState.LISTENING, words = "words", cue = true, spoken = false,
    pushToTalk = false, muted = false, paused = false, halfDuplex = false, pressed = false, kaiMuted = false,
    stoppable = false, heldBy = null, takingOver = false, pendingNote = null, waitingFor = null, threadId = thread,
    threadTitle = "Kai", tool = null, awaiting = awaiting, hold = false, holdFallback = null, heldStage = null,
    error = null, note = null, end = null,
)

/** The approval card's decisions (design §11). */
class ApprovalRulesTest {
    @Test
    fun aCardAnnouncesItselfUnlessTheCallSaysItAlready() {
        assertTrue(ApprovalRules.announces(card("a"), null))
        // The bound thread's call waits: the ring's live region says it.
        assertFalse(ApprovalRules.announces(card("a"), call(CallPhase.LIVE, 7, listOf("desktop-a__open_file"))))
        // Another thread's call, or a call that is not live: the card says it.
        assertTrue(ApprovalRules.announces(card("a"), call(CallPhase.LIVE, 8, listOf("desktop-a__open_file"))))
        assertTrue(ApprovalRules.announces(card("a"), call(CallPhase.ENDED, 7, emptyList())))
    }

    @Test
    fun aCardAsksUntilAVerdictIsHeldOrSent() {
        assertEquals(ApprovalRules.Status.ASK, ApprovalRules.status(card("a")))
        assertEquals(ApprovalRules.Status.HELD, ApprovalRules.status(card("a", verdict = true)))
        assertEquals(ApprovalRules.Status.SENT, ApprovalRules.status(card("a", verdict = false, sent = true)))
    }

    @Test
    fun aVoiceSurfaceShowsTheFirstCardThatStillAsksWithItsPlace() {
        assertNull(ApprovalRules.pick(emptyList()))
        val p = ApprovalRules.pick(listOf(card("a", verdict = true), card("b"), card("c")))!!
        assertEquals("b" to (2 to 3), p.card.approvalRequestId to (p.position to p.count))
        // Every verdict given: the first, to show what became of it.
        val held = ApprovalRules.pick(listOf(card("a", verdict = true, sent = true)))!!
        assertEquals("a", held.card.approvalRequestId)
    }

    @Test
    fun theConversationPutsEachCardAtItsMessageAndTheRestAtTheEnd() {
        // A call the session showed before the feed named its message (0), another thread's (99).
        val placement = ApprovalRules.place(
            listOf(card("a", 30), card("b", 0, inCall = true), card("c", 30), card("d", 99)),
            listOf(28, 29, 30),
        )
        assertEquals(listOf("a", "c"), placement.after[30L]!!.map { it.approvalRequestId })
        assertEquals(listOf("b", "d"), placement.end.map { it.approvalRequestId })
    }

    @Test
    fun theVoicePaneShowsACardOnACompactWindowOnly() {
        assertTrue(ApprovalRules.onVoicePane(Layout.Compact))
        // The conversation beside it shows the cards on a wide one: never two asks on one screen.
        assertFalse(ApprovalRules.onVoicePane(Layout.Wide))
    }

    @Test
    fun theOverlaySeesTheCardsOnlyShowingAndUnlocked() {
        assertTrue(ApprovalRules.overlaySees(showing = true, locked = false))
        // Locked: no card and no argument on the keyguard, so the notification asks (§9.6, §12.3).
        assertFalse(ApprovalRules.overlaySees(showing = true, locked = true))
        assertFalse(ApprovalRules.overlaySees(showing = false, locked = false))
    }
}
