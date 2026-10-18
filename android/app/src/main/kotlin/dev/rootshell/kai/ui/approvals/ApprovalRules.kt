package dev.rootshell.kai.ui.approvals

import dev.rootshell.kai.core.ffi.ApprovalView
import dev.rootshell.kai.core.ffi.CallPhase
import dev.rootshell.kai.core.ffi.VoiceView
import dev.rootshell.kai.ui.Layout

/**
 * The approval card's decisions (design §11), pure so the JVM tests run them; the core's book says
 * what waits, these say where and how a card shows it.
 *
 * - **One ask per call, wherever it shows.** The cards come from the hub's one list (the core
 *   merged the feed's and the call's by id), so the overlay, the voice screen and the conversation
 *   show the same card with the same state, and while one is on screen the core raises no
 *   notification for it (the app visible, or the overlay showing unlocked: [overlaySees]).
 * - **Voice surfaces show one card**, the first that still asks, with its place ("2 of 3"); on a
 *   wide window the voice pane shows none, since the conversation beside it shows each at its
 *   message ([onVoicePane]).
 * - **The conversation** puts each card after its message, and a card whose message it does not
 *   hold (another thread's, or a call the session showed before the feed named its message) after
 *   the last one ([place]).
 * - **The lock screen** shows no card and no tool argument: the overlay's locked face says only
 *   "Kai needs your approval" with Unlock, and deciding needs the unlock (§9.6, §12.3).
 */
object ApprovalRules {
    /** What a card asks: a verdict, nothing (a verdict held for the reply's others), or nothing (sent). */
    enum class Status { ASK, HELD, SENT }

    fun status(card: ApprovalView): Status = when {
        card.sent -> Status.SENT
        card.verdict != null -> Status.HELD
        else -> Status.ASK
    }

    /** The card a voice surface shows, and its place among the cards (1-based). */
    data class Pick(val card: ApprovalView, val position: Int, val count: Int)

    fun pick(cards: List<ApprovalView>): Pick? {
        if (cards.isEmpty()) return null
        val i = cards.indexOfFirst { status(it) == Status.ASK }.takeIf { it >= 0 } ?: 0
        return Pick(cards[i], i + 1, cards.size)
    }

    /** The voice pane shows a card on a compact window only (e.g. a foldable's cover screen). */
    fun onVoicePane(layout: Layout): Boolean = layout == Layout.Compact

    /**
     * A voice surface's card announces itself (a polite live region) unless the call already says
     * it: the ring's live region carries "Kai waits for your decision on …" for its own thread's
     * calls, and TalkBack should hear one event once (§10.3's note).
     */
    fun announces(card: ApprovalView, call: VoiceView?): Boolean =
        !(call != null && call.phase == CallPhase.LIVE && call.threadId == card.threadId && call.awaiting.isNotEmpty())

    /** The overlay shows the cards, and so counts as seeing them, only while it shows unlocked. */
    fun overlaySees(showing: Boolean, locked: Boolean): Boolean = showing && !locked

    /** Where the conversation puts the cards: after their message by id, the rest at the end. */
    data class Placement(val after: Map<Long, List<ApprovalView>>, val end: List<ApprovalView>)

    fun place(cards: List<ApprovalView>, messageIds: List<Long>): Placement {
        val ids = messageIds.toSet()
        val (at, rest) = cards.partition { it.messageId in ids }
        return Placement(at.groupBy { it.messageId }, rest)
    }
}
