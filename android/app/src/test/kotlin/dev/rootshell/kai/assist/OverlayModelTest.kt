package dev.rootshell.kai.assist

import dev.rootshell.kai.R
import dev.rootshell.kai.core.ffi.CallEnd
import dev.rootshell.kai.core.ffi.CallPhase
import dev.rootshell.kai.core.ffi.CallStart
import dev.rootshell.kai.core.ffi.EndReason
import dev.rootshell.kai.core.ffi.RingState
import dev.rootshell.kai.core.ffi.VoiceAction
import dev.rootshell.kai.core.ffi.VoiceView
import dev.rootshell.kai.voice.Started
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import kotlinx.coroutines.runBlocking
import org.junit.Test

private fun view(call: Long, phase: CallPhase, spoken: Boolean = false, end: CallEnd? = null) = VoiceView(
    call = call.toULong(), phase = phase, ring = RingState.LISTENING, words = "Listening", cue = true, spoken = spoken,
    pushToTalk = false, muted = false, paused = false, halfDuplex = false, pressed = false, kaiMuted = false,
    stoppable = false, heldBy = null, takingOver = false, pendingNote = null, waitingFor = null, threadId = 7,
    threadTitle = "Kai", tool = null, awaiting = emptyList(), hold = false, holdFallback = null, heldStage = null, error = null, note = null,
    end = end,
)

private fun facts(call: VoiceView? = null, locked: Boolean = false, microphone: Boolean = true) =
    ShowFacts(locked = locked, microphone = microphone, call = call)

private val TALK = Show.Start(CallStart(new = false, takeOver = false))

/** The assistant overlay's decisions (design §9.2, §9.6, §7.7). */
class OverlayModelTest {
    /** A model whose show started call `id`, answered. */
    private fun startedCall(id: Long): OverlayModel = OverlayModel().also {
        assertEquals(TALK, it.shown(facts()))
        assertNull(it.answered(Started.Call(id.toULong())))
    }

    @Test
    fun aShowStartsACallUnlessOneRunsTheLockOrTheMicrophoneSaysNo() {
        assertEquals(TALK, OverlayModel().shown(facts()))
        // An ended call is no call: a new one starts.
        assertEquals(TALK, OverlayModel().shown(facts(view(3, CallPhase.ENDED))))
        // onShow can repeat while a call runs: it shows that one, starting nothing.
        for (phase in listOf(CallPhase.CONNECTING, CallPhase.PENDING, CallPhase.LIVE, CallPhase.ENDING)) {
            assertEquals(Show.Running, OverlayModel().shown(facts(view(3, phase))))
        }
        // Locked: nothing starts (§9.6), a running call included.
        val locked = OverlayModel()
        assertEquals(Show.Locked, locked.shown(facts(locked = true)))
        assertEquals(Blocked.Locked, locked.blocked)
        assertEquals(Show.Locked, OverlayModel().shown(facts(view(3, CallPhase.LIVE), locked = true)))
        // The overlay cannot ask for the microphone: Open Kai does.
        val mic = OverlayModel()
        assertEquals(Show.NeedsMicrophone, mic.shown(facts(microphone = false)))
        assertEquals(Blocked.Microphone, mic.blocked)
    }

    @Test
    fun aShowWhileItsStartIsOnItsWayStartsNothingMore() {
        val m = OverlayModel()
        assertEquals(TALK, m.shown(facts()))
        assertTrue(m.starting)
        // The core has not published the call yet; the system shows the overlay again.
        assertEquals(Show.Running, m.shown(facts()))
        m.answered(Started.Call(4uL))
        assertFalse(m.starting)
    }

    @Test
    fun anUnlockWhileTheLockedFaceShowsLetsTheInvocationGoOn() {
        val m = OverlayModel()
        assertEquals(Show.Locked, m.shown(facts(locked = true)))
        assertEquals(TALK, m.unlocked(facts()))
        // Only the locked face's invocation goes on, and only while the overlay shows.
        assertNull(m.unlocked(facts()))
        val hidden = OverlayModel()
        hidden.shown(facts(locked = true))
        hidden.hidden()
        assertNull(hidden.unlocked(facts()))
    }

    @Test
    fun aDismissalNamesTheOverlaysOwnRunningCallForTheCoreToDecide() {
        // The core decides what it does (before the first turn it ends, after it it goes on): the
        // overlay names its call whatever the turn, so a commit in flight is never raced.
        for (spoken in listOf(false, true)) {
            val m = startedCall(4)
            assertEquals(4uL, m.hidden())
            assertFalse(m.showing)
        }
        // A pending bind the overlay started: the core declines it.
        assertEquals(4uL, startedCall(4).hidden())
        // The overlay names its call whatever the call is here: the core ignores an ended call
        // and a number that is not the current one, so a dismissal never races the new call's view.
        // No call of the overlay's: nothing to name.
        assertNull(OverlayModel().also { it.shown(facts(view(5, CallPhase.LIVE))) }.hidden())
    }

    @Test
    fun aCallTheAppStartedIsNeverEndedByTheOverlay() {
        val m = OverlayModel()
        assertEquals(Show.Running, m.shown(facts(view(9, CallPhase.LIVE))))
        assertNull(m.hidden())
        // An overlay that showed its own call earlier and the app's now: still not the app's call.
        val own = startedCall(4)
        own.hidden()
        assertEquals(Show.Running, own.shown(facts(view(9, CallPhase.LIVE))))
        // The old number is named; the core ignores it (the current call is 9).
        assertEquals(4uL, own.hidden())
    }

    @Test
    fun aDismissalBeforeTheStartIsAnsweredDismissesTheCallAtTheAnswer() {
        val m = OverlayModel()
        assertEquals(TALK, m.shown(facts()))
        // Swiped away at once, before the core answered the start.
        assertNull(m.hidden())
        assertEquals(6uL, m.answered(Started.Call(6uL)))
        // A refusal needs nothing dismissed.
        val refused = OverlayModel()
        refused.shown(facts())
        refused.hidden()
        assertNull(refused.answered(Started.Refused("Not paired with lmgw")))
        // An answer while the overlay still shows dismisses nothing.
        val shown = OverlayModel()
        shown.shown(facts())
        assertNull(shown.answered(Started.Call(7uL)))
        assertEquals(7uL, shown.started)
    }

    @Test
    fun aShowAfterADismissalWhileStartingForgetsTheDismissal() {
        val m = OverlayModel()
        assertEquals(TALK, m.shown(facts()))
        assertNull(m.hidden())
        // Shown again before the core answered: the start's call is wanted, so it is not dismissed.
        assertEquals(Show.Running, m.shown(facts()))
        assertNull(m.answered(Started.Call(6uL)))
        assertEquals(6uL, m.started)
        // An earlier call of the overlay's is named too while a new start is on its way, and the
        // new call is dismissed at its answer.
        val n = startedCall(4)
        n.hidden()
        assertEquals(TALK, n.shown(facts(view(4, CallPhase.ENDED))))
        assertEquals(4uL, n.hidden())
        assertEquals(5uL, n.answered(Started.Call(5uL)))
    }

    @Test
    fun expandHandsTheCallOverWithoutADismissalOrANewCall() {
        val m = startedCall(4)
        m.handover()
        // The hide that follows is no dismissal: the call goes on in the app.
        assertNull(m.hidden())
        // Keyboard opens the conversation, likewise without a dismissal.
        val k = startedCall(5)
        k.handover()
        assertNull(k.hidden())
        // Expand while the start is on its way: the answer dismisses nothing either.
        val early = OverlayModel()
        early.shown(facts())
        early.handover()
        assertNull(early.hidden())
        assertNull(early.answered(Started.Call(8uL)))
        // After Expand the call is the app's: a later show of the overlay does not end it.
        assertEquals(Show.Running, m.shown(facts(view(4, CallPhase.LIVE))))
        assertNull(m.hidden())
        assertEquals(Show.Running, early.shown(facts(view(8, CallPhase.LIVE))))
        assertNull(early.hidden())
        // A new call the overlay starts is its own again.
        assertEquals(TALK, m.shown(facts(view(4, CallPhase.ENDED))))
        m.answered(Started.Call(9uL))
        assertEquals(9uL, m.hidden())
    }

    @Test
    fun expandTriesStartActivityFirstThenTheAssistantsOwn() {
        // One Kai in Recents (design item 23): the session's own task is the fallback (WP0 (a) may reorder).
        assertEquals(listOf(ExpandPath.START_ACTIVITY, ExpandPath.ASSISTANT_ACTIVITY), EXPAND_PATHS)
    }

    @Test
    fun expandGoesThroughThePathsInOrderAndNoneThatOpenedMeansNoHandover() = runBlocking {
        val tried = mutableListOf<ExpandPath>()
        // The first path threw or the app never showed: the next one opens it.
        assertEquals(
            ExpandPath.ASSISTANT_ACTIVITY,
            expandThrough(EXPAND_PATHS) { tried += it; it == ExpandPath.ASSISTANT_ACTIVITY },
        )
        assertEquals(EXPAND_PATHS, tried)
        // The first opens it: the rest are never tried.
        tried.clear()
        assertEquals(ExpandPath.START_ACTIVITY, expandThrough(EXPAND_PATHS) { tried += it; true })
        assertEquals(listOf(ExpandPath.START_ACTIVITY), tried)
        // None: null, so the session keeps the sheet with an error and the model never hands over.
        assertNull(expandThrough(EXPAND_PATHS) { false })
        // The overlay still owns the call it started, and a dismissal still names it.
        val m = startedCall(4)
        assertEquals(4uL, m.started)
        assertEquals(4uL, m.hidden())
    }

    @Test
    fun theScreenGoingOffHidesTheOverlayEverythingElseJustSyncs() {
        assertEquals(LockStep.Hide, lockStep("android.intent.action.SCREEN_OFF"))
        assertEquals(LockStep.Sync, lockStep("android.intent.action.SCREEN_ON"))
        assertEquals(LockStep.Sync, lockStep("android.intent.action.USER_PRESENT"))
        assertEquals(LockStep.Sync, lockStep(null))
    }

    @Test
    fun aRefusedStartIsShownInTheCoresWordsAndTalkTriesAgain() {
        val m = OverlayModel()
        m.shown(facts())
        m.answered(Started.Refused("Not paired with lmgw"))
        assertEquals(Blocked.Words("Not paired with lmgw"), m.blocked)
        // Take over from an ended call's words, in the overlay.
        val takeOver = CallStart(new = false, takeOver = true)
        assertEquals(Show.Start(takeOver), m.talk(facts(view(2, CallPhase.ENDED)), takeOver))
        assertNull(m.blocked)
    }

    @Test
    fun theFaceFollowsTheLockTheCallAndTheStart() {
        val end = CallEnd(EndReason.CONNECTION_LOST, "connection lost (failed)", null, null, listOf(VoiceAction.RECONNECT), null)
        // Locked: nothing of the conversation, a running call's controls only.
        assertEquals(Face.Locked(call = true), face(locked = true, blocked = null, starting = false, view = view(1, CallPhase.LIVE)))
        assertEquals(Face.Locked(call = false), face(locked = true, blocked = Blocked.Locked, starting = false, view = null))
        // A running call wins over a stale refusal.
        assertEquals(Face.Call, face(false, Blocked.Words("a call is running"), false, view(1, CallPhase.LIVE)))
        assertEquals(Face.Blocked(Blocked.Microphone), face(false, Blocked.Microphone, false, null))
        // While the overlay's start is on its way, an older call's end is not shown.
        assertEquals(Face.Starting, face(false, null, true, view(1, CallPhase.ENDED, end = end)))
        assertEquals(Face.Ended(end), face(false, null, false, view(1, CallPhase.ENDED, end = end)))
        assertEquals(Face.Starting, face(false, null, false, null))
    }

    @Test
    fun theHintSaysWhatClosingDoesToTheCall() {
        assertEquals(R.string.overlay_hint_ends, dismissalHint(view(4, CallPhase.LIVE), started = 4uL))
        assertEquals(R.string.overlay_hint_goes_on, dismissalHint(view(4, CallPhase.LIVE, spoken = true), started = 4uL))
        assertEquals(R.string.overlay_hint_declines, dismissalHint(view(4, CallPhase.PENDING), started = 4uL))
        // The app's call goes on whatever the turn.
        assertEquals(R.string.overlay_hint_goes_on, dismissalHint(view(9, CallPhase.LIVE), started = 4uL))
        assertNull(dismissalHint(view(4, CallPhase.ENDED), started = 4uL))
        assertNull(dismissalHint(null, started = null))
    }
}
