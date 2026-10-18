package dev.rootshell.kai.app

import dev.rootshell.kai.FakeCore
import dev.rootshell.kai.core.ffi.CallEnd
import dev.rootshell.kai.core.ffi.CallPhase
import dev.rootshell.kai.core.ffi.CaptionLine
import dev.rootshell.kai.core.ffi.ConnectionAction
import dev.rootshell.kai.core.ffi.ConnectionState
import dev.rootshell.kai.core.ffi.ConnectionView
import dev.rootshell.kai.core.ffi.MessageView
import dev.rootshell.kai.core.ffi.MessagesDelta
import dev.rootshell.kai.core.ffi.NotificationId
import dev.rootshell.kai.core.ffi.NotificationKind
import dev.rootshell.kai.core.ffi.NotificationView
import dev.rootshell.kai.core.ffi.EndReason
import dev.rootshell.kai.core.ffi.RingState
import dev.rootshell.kai.core.ffi.Role
import dev.rootshell.kai.core.ffi.Speaker
import dev.rootshell.kai.core.ffi.Update
import dev.rootshell.kai.core.ffi.VoiceAction
import dev.rootshell.kai.core.ffi.VoiceView
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

private fun message(id: Long, text: String, streaming: Boolean = false, notices: List<String> = emptyList()) =
    MessageView(id, Role.ASSISTANT, text, emptyList(), "", null, false, streaming, emptyList(), null, notices)

private fun view(state: ConnectionState, paired: Boolean) = ConnectionView(
    state, "words", emptyList(), listOf(ConnectionAction.TRY_AGAIN), null, paired, null, null, null, false, null, null, null,
)

private class Sink : HubSink {
    val log = mutableListOf<String>()
    override fun notify(notification: NotificationView) { log += "notify ${NotificationTags.tag(notification.id)}" }
    override fun withdraw(id: NotificationId) { log += "withdraw ${NotificationTags.tag(id)}" }
    override fun wakeAt(bootMs: ULong?) { log += "wake $bootMs" }
    override fun connection(previous: ConnectionView?, view: ConnectionView, visible: Boolean) {
        log += "connection ${previous?.state} -> ${view.state} visible=$visible"
    }
    override fun notice(text: String) { log += "notice $text" }
    override fun voice(previous: VoiceView?, view: VoiceView, visible: Boolean) {
        log += "voice ${previous?.phase} -> ${view.phase} visible=$visible"
    }
}

private fun caption(call: Long, id: Long, text: String) =
    CaptionLine(call.toULong(), id.toULong(), Speaker.KAI, text, false, null)

private fun voiceView(call: Long, phase: CallPhase, end: CallEnd? = null) = VoiceView(
    call = call.toULong(), phase = phase, ring = RingState.REST, words = "words", cue = false, spoken = false, pushToTalk = false,
    muted = false, paused = false, halfDuplex = false, pressed = false, kaiMuted = false, stoppable = false,
    heldBy = null, takingOver = false, pendingNote = null, waitingFor = null, threadId = 7, threadTitle = "Kai",
    tool = null, awaiting = emptyList(), hold = false, holdFallback = null, heldStage = null, error = null, note = null, end = end,
)

class HubTest {
    @Test
    fun messagesMergeByIdWithTheStreamingReplyLast() {
        var list = MessageList.apply(emptyList(), MessagesDelta(true, 7, listOf(message(2, "b"), message(1, "a")), emptyList()))
        assertEquals(listOf(1L, 2L), list.map { it.id })
        list = MessageList.apply(list, MessagesDelta(false, 7, listOf(message(-1, "He", streaming = true), message(3, "c")), emptyList()))
        assertEquals(listOf(1L, 2L, 3L, -1L), list.map { it.id })
        list = MessageList.apply(list, MessagesDelta(false, 7, listOf(message(4, "Hello")), listOf(-1L)))
        assertEquals(listOf(1L, 2L, 3L, 4L), list.map { it.id })
        list = MessageList.apply(list, MessagesDelta(true, 8, emptyList(), emptyList()))
        assertTrue(list.isEmpty())
    }

    @Test
    fun aSavedReplyKeepsItsNoticesWhereTheStreamingOneHadThem() {
        val notice = "Kai desktop is offline. Answered without its tools."
        var list = MessageList.apply(emptyList(), MessagesDelta(true, 7, emptyList(), emptyList()))
        list = MessageList.apply(list, MessagesDelta(false, 7, listOf(message(-1, "Hi", streaming = true, notices = listOf(notice))), emptyList()))
        list = MessageList.apply(list, MessagesDelta(false, 7, listOf(message(11, "Hi", notices = listOf(notice))), listOf(-1L)))
        assertEquals(listOf(11L), list.map { it.id })
        assertEquals(listOf(notice), list.single().notices)
    }

    @Test
    fun captionsMergeByIdAndANewCallStartsAgain() {
        var lines = CaptionList.apply(emptyList(), caption(1, 1, "Hel"))
        lines = CaptionList.apply(lines, caption(1, 2, "lo"))
        lines = CaptionList.apply(lines, caption(1, 1, "Hello"))
        assertEquals(listOf("Hello", "lo"), lines.map { it.text })
        // An older call's line is dropped; a newer call's starts the list again.
        assertEquals(lines, CaptionList.apply(lines, caption(0, 9, "old")))
        lines = CaptionList.apply(lines, caption(2, 1, "new call"))
        assertEquals(listOf("new call"), lines.map { it.text })
    }

    @Test
    fun theHubHoldsTheCallAndItsCaptions() {
        val sink = Sink()
        val hub = CoreHub(FakeCore(), sink)
        hub.apply(Update.Voice(voiceView(1, CallPhase.CONNECTING)))
        hub.apply(Update.Caption(caption(1, 1, "Hi")))
        val end = CallEnd(EndReason.TAKEN_OVER, "Voice moved to device 'desktop'", null, "device 'desktop'", listOf(VoiceAction.TAKE_OVER), null)
        hub.apply(Update.Voice(voiceView(1, CallPhase.ENDED, end)))
        assertEquals(CallPhase.ENDED, hub.voice.value?.phase)
        assertEquals(listOf("Hi"), hub.captions.value.map { it.text })
        assertEquals(
            listOf("voice null -> CONNECTING visible=false", "voice CONNECTING -> ENDED visible=false"),
            sink.log,
        )
    }

    @Test
    fun theHubHoldsStateAndHandsThePlatformItsPart() {
        val core = FakeCore()
        val sink = Sink()
        val hub = CoreHub(core, sink)
        hub.apply(Update.Connection(view(ConnectionState.CONNECTING, true)))
        hub.apply(Update.Connection(view(ConnectionState.CONNECTED, true)))
        assertEquals(ConnectionState.CONNECTED, hub.connection.value?.state)
        val n = NotificationView(NotificationId(NotificationKind.APPROVAL, "mcpr_1"), "t", "x", 7, null, 1u)
        hub.apply(Update.Notify(n))
        hub.apply(Update.Withdraw(n.id))
        hub.apply(Update.WakeAt(1234uL))
        hub.apply(Update.Notice("lmgw did not take the decision"))
        hub.setVisible(true)
        hub.apply(Update.Notice("shown on screen"))
        assertEquals(
            listOf(
                "connection null -> CONNECTING visible=false",
                "connection CONNECTING -> CONNECTED visible=false",
                "notify approval:mcpr_1",
                "withdraw approval:mcpr_1",
                "wake 1234",
                "notice lmgw did not take the decision",
                "connection CONNECTED -> CONNECTED visible=true",
            ),
            sink.log,
        )
        assertEquals(listOf("foreground true"), core.calls)
    }
}
