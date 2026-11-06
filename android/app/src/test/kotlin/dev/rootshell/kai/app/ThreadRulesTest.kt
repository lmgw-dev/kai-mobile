package dev.rootshell.kai.app

import dev.rootshell.kai.FakeCore
import dev.rootshell.kai.core.ffi.BrowserStatus
import dev.rootshell.kai.core.ffi.BrowserView
import dev.rootshell.kai.core.ffi.CallEnd
import dev.rootshell.kai.core.ffi.CallPhase
import dev.rootshell.kai.core.ffi.CallStart
import dev.rootshell.kai.core.ffi.CoreException
import dev.rootshell.kai.core.ffi.EndReason
import dev.rootshell.kai.core.ffi.FolderChoice
import dev.rootshell.kai.core.ffi.MessageView
import dev.rootshell.kai.core.ffi.MessagesDelta
import dev.rootshell.kai.core.ffi.OpenedStatus
import dev.rootshell.kai.core.ffi.OpenedView
import dev.rootshell.kai.core.ffi.RingState
import dev.rootshell.kai.core.ffi.Role
import dev.rootshell.kai.core.ffi.SearchPageView
import dev.rootshell.kai.core.ffi.SearchThreadView
import dev.rootshell.kai.core.ffi.Update
import dev.rootshell.kai.core.ffi.VoiceView
import dev.rootshell.kai.voice.active
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

private fun hit(id: Long) = SearchThreadView(id, "Thread $id", null, false, false, 1, emptyList())

private fun opened(id: Long, temporary: Boolean = false, kai: Boolean = false, status: OpenedStatus = OpenedStatus.READY) = OpenedView(
    status, id, "Recipes", 2, false, false, temporary, kai, null, emptyList(), null, false,
)

private fun call(phase: CallPhase, bound: Long? = null, title: String = "Kai", switching: CallStart? = null, end: CallEnd? = null) = VoiceView(
    call = 1uL, phase = phase, ring = RingState.LISTENING, words = "Listening", cue = true, spoken = false, pushToTalk = false,
    muted = false, paused = false, halfDuplex = false, pressed = false, kaiMuted = false, stoppable = false,
    heldBy = null, takingOver = false, pendingNote = null, waitingFor = null, bindThread = bound, switching = switching,
    threadId = bound ?: 7, threadTitle = title, tool = null, awaiting = emptyList(), hold = false,
    heldStage = null, fallbackStages = emptyList(), error = null, note = null, end = end,
)

/** The end of a call whose move (or rebind) goes to `thread`: the restart it owes. */
private fun moved(thread: Long?) = CallEnd(EndReason.REBIND, null, null, null, emptyList(), CallStart(new = false, takeOver = false, thread = thread))

private object NullSink : HubSink {
    override fun notify(notification: dev.rootshell.kai.core.ffi.NotificationView) {}
    override fun withdraw(id: dev.rootshell.kai.core.ffi.NotificationId) {}
    override fun wakeAt(bootMs: ULong?) {}
    override fun connection(previous: dev.rootshell.kai.core.ffi.ConnectionView?, view: dev.rootshell.kai.core.ffi.ConnectionView, visible: Boolean) {}
    override fun notice(text: String) {}
    override fun voice(previous: dev.rootshell.kai.core.ffi.VoiceView?, view: dev.rootshell.kai.core.ffi.VoiceView, visible: Boolean) {}
}

class ThreadRulesTest {
    @Test
    fun aThreadsMenuFollowsItsStateAndATemporaryThreadHasNone() {
        assertEquals(
            listOf(ThreadAction.PIN, ThreadAction.ARCHIVE, ThreadAction.MOVE, ThreadAction.RENAME, ThreadAction.DELETE),
            ThreadRules.actions(pinned = false, archived = false, temporary = false),
        )
        assertEquals(
            listOf(ThreadAction.UNPIN, ThreadAction.RESTORE, ThreadAction.MOVE, ThreadAction.RENAME, ThreadAction.DELETE),
            ThreadRules.actions(pinned = true, archived = true, temporary = false),
        )
        assertTrue(ThreadRules.actions(pinned = false, archived = false, temporary = true).isEmpty())
    }

    @Test
    fun aMoveOffersTheOtherFoldersAndNoFolder() {
        val folders = listOf(FolderChoice(1, "Kai"), FolderChoice(2, "Notes"))
        assertEquals(listOf(MoveTarget(1, "Kai"), MoveTarget(null, null)), ThreadRules.moveTargets(folders, 2))
        // A thread in no folder cannot move to no folder.
        assertEquals(listOf(MoveTarget(1, "Kai"), MoveTarget(2, "Notes")), ThreadRules.moveTargets(folders, null))
    }

    @Test
    fun searchesStartAtTwoCharactersAndPagesAppendOnce() {
        assertFalse(ThreadRules.searchable(" a "))
        assertTrue(ThreadRules.searchable("ab"))
        val shown = listOf(hit(1), hit(2))
        val page = SearchPageView(4, listOf(hit(2), hit(3)), null)
        assertEquals(listOf(1L, 2L, 3L), ThreadRules.append(shown, page).map { it.threadId })
    }

    @Test
    fun withoutACallTalkBindsTheThreadButNotATemporaryOne() {
        assertEquals(ThreadVoice.Idle(talk = true), ThreadRules.voice(null, opened(12)))
        assertEquals(ThreadVoice.Idle(talk = true), ThreadRules.voice(call(CallPhase.ENDED, bound = 12), opened(12)))
        assertEquals(ThreadVoice.Idle(talk = false), ThreadRules.voice(call(CallPhase.OFF), opened(-3, temporary = true)))
    }

    @Test
    fun aCallBoundElsewhereNamesWhereWithTalkHere() {
        // The Kai conversation's call (bound by the folder): no title, Back to Kai.
        assertEquals(
            ThreadVoice.Elsewhere(threadId = null, title = null, coming = false, talk = true),
            ThreadRules.voice(call(CallPhase.LIVE), opened(12)),
        )
        assertEquals(
            ThreadVoice.Elsewhere(threadId = null, title = null, coming = false, talk = true),
            ThreadRules.voice(call(CallPhase.CONNECTING), opened(12)),
        )
        // Another thread's call, named once lmgw named it.
        assertEquals(
            ThreadVoice.Elsewhere(threadId = 13, title = "Example plans", coming = false, talk = true),
            ThreadRules.voice(call(CallPhase.LIVE, bound = 13, title = "Example plans"), opened(12)),
        )
        assertEquals(
            ThreadVoice.Elsewhere(threadId = 13, title = null, coming = false, talk = true),
            ThreadRules.voice(call(CallPhase.CONNECTING, bound = 13, title = ""), opened(12)),
        )
        // A Talk here waits for the call's quiet point.
        assertEquals(
            ThreadVoice.Elsewhere(threadId = null, title = null, coming = true, talk = true),
            ThreadRules.voice(call(CallPhase.LIVE, switching = CallStart(new = false, takeOver = false, thread = 12)), opened(12)),
        )
        // A temporary thread offers no Talk here.
        assertFalse((ThreadRules.voice(call(CallPhase.LIVE), opened(-3, temporary = true)) as ThreadVoice.Elsewhere).talk)
    }

    @Test
    fun theBoundThreadSaysSoAndWhetherTheCallLeaves() {
        assertEquals(ThreadVoice.Here(leaving = false), ThreadRules.voice(call(CallPhase.LIVE, bound = 12), opened(12)))
        assertEquals(
            ThreadVoice.Here(leaving = true),
            ThreadRules.voice(call(CallPhase.LIVE, bound = 12, switching = CallStart(new = false, takeOver = false, thread = null)), opened(12)),
        )
    }

    @Test
    fun aGoneThreadOffersNoTalk() {
        val gone = opened(12, status = OpenedStatus.GONE)
        assertEquals(ThreadVoice.Idle(talk = false), ThreadRules.voice(null, gone))
        assertFalse((ThreadRules.voice(call(CallPhase.LIVE), gone) as ThreadVoice.Elsewhere).talk)
    }

    @Test
    fun theKaiThreadOpenedInTheBrowserIsWhereTheKaiCallTalks() {
        val kai = opened(7, kai = true)
        assertEquals(ThreadVoice.Here(leaving = false), ThreadRules.voice(call(CallPhase.LIVE), kai))
        assertTrue(ThreadRules.sendStopsVoice(call(CallPhase.LIVE), 7, kai = true))
        assertFalse(ThreadRules.sendStopsVoice(call(CallPhase.LIVE, bound = 12), 7, kai = true))
        // A thread's call moving to the Kai conversation comes here.
        assertEquals(
            ThreadVoice.Elsewhere(threadId = 12, title = "Example plans", coming = true, talk = true),
            ThreadRules.voice(call(CallPhase.LIVE, bound = 12, title = "Example plans", switching = CallStart(new = false, takeOver = false, thread = null)), kai),
        )
    }

    /**
     * Between a move's two calls (the old one ended, its restart not yet a call) the call still
     * runs, bound to the target: no thread offers Talk, which would start a second call (review of
     * 2026-10-19).
     */
    @Test
    fun whileAMoveGoesTheTargetHasTheCallAndNoThreadOffersTalk() {
        for (phase in listOf(CallPhase.ENDING, CallPhase.ENDED)) {
            val gap = call(phase, bound = 13, title = "Example plans", end = moved(12))
            assertTrue(gap.active())
            assertEquals(ThreadVoice.Here(leaving = false), ThreadRules.voice(gap, opened(12)))
            assertEquals(
                ThreadVoice.Elsewhere(threadId = 12, title = null, coming = false, talk = false),
                ThreadRules.voice(gap, opened(13)),
            )
            assertFalse(ThreadRules.sendStopsVoice(gap, 13))
        }
        // A rebind of the Kai call by the folder: the Kai conversation's, still no Talk elsewhere.
        val rebind = call(CallPhase.ENDED, end = moved(null))
        assertEquals(
            ThreadVoice.Elsewhere(threadId = null, title = null, coming = false, talk = false),
            ThreadRules.voice(rebind, opened(12)),
        )
        // An end with no restart owed is over: Talk again.
        val over = call(CallPhase.ENDED, bound = 13, end = moved(12).copy(restart = null))
        assertFalse(over.active())
        assertEquals(ThreadVoice.Idle(talk = true), ThreadRules.voice(over, opened(12)))
    }

    @Test
    fun aSendStopsTheVoiceTurnOnlyInTheBoundConversation() {
        assertTrue(ThreadRules.sendStopsVoice(call(CallPhase.LIVE), null))
        assertFalse(ThreadRules.sendStopsVoice(call(CallPhase.LIVE), 12))
        assertTrue(ThreadRules.sendStopsVoice(call(CallPhase.LIVE, bound = 12), 12))
        assertFalse(ThreadRules.sendStopsVoice(call(CallPhase.LIVE, bound = 12), null))
        assertFalse(ThreadRules.sendStopsVoice(call(CallPhase.ENDED, bound = 12), 12))
        assertFalse(ThreadRules.sendStopsVoice(null, null))
    }

    @Test
    fun aRefusalShowsLmgwsWordsWithItsCode() {
        assertEquals(
            "a temporary chat cannot be pinned (temporary_thread)",
            ThreadRules.words(CoreException.Refused("temporary_thread", "a temporary chat cannot be pinned", null)),
        )
        assertEquals(
            "busy (key_rate): a voice call holds a slot",
            ThreadRules.words(CoreException.Refused("key_rate", "busy", "a voice call holds a slot")),
        )
        assertEquals("not connected", ThreadRules.words(CoreException.NotConnected("not connected")))
    }

    @Test
    fun theHubOpensOneThreadAtATimeAndLeavingTheBrowserClosesIt() {
        val core = FakeCore()
        val hub = CoreHub(core, NullSink)
        hub.showBrowser(true)
        hub.showBrowser(true)
        hub.select(ThreadTarget(5))
        hub.select(ThreadTarget(5, messageId = 41))
        hub.select(ThreadTarget(6))
        hub.apply(Update.Opened(opened(6)))
        val m = MessageView(1, Role.USER, "hi", emptyList(), "", null, false, false, emptyList(), null, emptyList())
        hub.apply(Update.OpenedMessages(MessagesDelta(true, 6, listOf(m), emptyList())))
        assertEquals(listOf(1L), hub.openedMessages.value.map { it.id })
        hub.apply(Update.Browser(BrowserView(BrowserStatus.READY, null, emptyList(), emptyList(), 0, null, emptyList())))
        assertEquals(BrowserStatus.READY, hub.browser.value?.status)
        hub.showBrowser(false)
        assertNull(hub.selected.value)
        assertNull(hub.browser.value)
        hub.apply(Update.Opened(null))
        assertTrue(hub.openedMessages.value.isEmpty())
        // A browser view after it closed is not kept.
        hub.apply(Update.Browser(BrowserView(BrowserStatus.READY, null, emptyList(), emptyList(), 0, null, emptyList())))
        assertNull(hub.browser.value)
        assertEquals(listOf("browser true", "open 5", "open 6", "browser false", "close"), core.calls)
    }
}
