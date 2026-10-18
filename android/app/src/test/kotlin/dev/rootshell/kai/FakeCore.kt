package dev.rootshell.kai

import dev.rootshell.kai.core.Colours
import dev.rootshell.kai.core.CoreBuild
import dev.rootshell.kai.core.CoreFacade
import dev.rootshell.kai.core.LmgwPin
import dev.rootshell.kai.core.VoiceFacade
import dev.rootshell.kai.core.ffi.AudioFocus
import dev.rootshell.kai.core.ffi.CallStart
import dev.rootshell.kai.core.ffi.Decided
import dev.rootshell.kai.core.ffi.EventSink
import dev.rootshell.kai.core.ffi.HeadsetPress
import dev.rootshell.kai.core.ffi.NetworkFacts
import dev.rootshell.kai.core.ffi.NotificationId
import dev.rootshell.kai.core.ffi.PairingPreview
import dev.rootshell.kai.core.ffi.PairingView
import dev.rootshell.kai.core.ffi.PlayoutStats
import dev.rootshell.kai.core.ffi.Settings
import dev.rootshell.kai.core.ffi.TransportState
import dev.rootshell.kai.core.ffi.Update
import kotlinx.coroutines.channels.Channel

/** The core as the tests fake it: the calls it got, and the updates it hands over. */
class FakeCore : CoreFacade {
    val updates = Channel<Update>(Channel.UNLIMITED)
    val calls = mutableListOf<String>()

    override fun version() = "0.1.0"
    override fun lmgwPin() = LmgwPin("0123456789abcdef0123456789abcdef01234567", "unpublished", "fedcba9876543210fedcba9876543210fedcba98")
    override fun build() = CoreBuild("release", "aarch64-linux-android")
    override fun colours() = Colours.Kai
    override fun settingsNotice(): String? = null
    override fun settings(): Settings = throw NotImplementedError("not used by these tests")
    override suspend fun setSettings(settings: Settings) { calls += "setSettings" }
    override fun paired() = false
    override suspend fun pairingPreview(text: String): PairingPreview = throw NotImplementedError()
    override suspend fun pair(text: String): PairingView = throw NotImplementedError()
    override suspend fun forget() { calls += "forget" }
    override fun retry() { calls += "retry" }
    override fun setNetwork(net: NetworkFacts) { calls += "network ${net.id}" }
    override fun setForeground(visible: Boolean) { calls += "foreground $visible" }
    override fun setOverlay(showingCards: Boolean) { calls += "overlay $showingCards" }
    override fun wake() { calls += "wake" }
    override suspend fun nextUpdate(): Update = updates.receive()
    override suspend fun sendText(text: String) { calls += "send $text" }
    override suspend fun newConversation() { calls += "new" }
    override suspend fun decide(approvalRequestId: String, approve: Boolean): Decided {
        calls += "decide $approvalRequestId $approve"
        return Decided.Sent
    }
    override suspend fun dismissApproval(approvalRequestId: String) { calls += "dismiss $approvalRequestId" }
    override fun notificationGone(id: NotificationId) { calls += "gone ${id.key}" }

    override val voice: VoiceFacade = object : VoiceFacade {
        override fun prepare() { calls += "voice prepare" }
        override suspend fun start(start: CallStart, sink: EventSink): ULong {
            calls += "voice start ${start.takeOver}"
            return 1uL
        }
        override suspend fun offer(call: ULong, sdp: String): String = "v=0"
        override fun transport(call: ULong, state: TransportState, detail: String) { calls += "voice transport $call $state" }
        override fun channelOpen(call: ULong) { calls += "voice open $call" }
        override fun channelMessage(call: ULong, text: String) { calls += "voice message $call" }
        override fun channelClosed(call: ULong, why: String) { calls += "voice closed $call" }
        override fun failed(call: ULong, words: String) { calls += "voice failed $call $words" }
        override fun takeOver() { calls += "voice take over" }
        override fun press() { calls += "voice press" }
        override fun release() { calls += "voice release" }
        override fun tap() { calls += "voice tap" }
        override fun stop() { calls += "voice stop" }
        override fun setMuted(muted: Boolean) { calls += "voice muted $muted" }
        override fun setPushToTalk(on: Boolean) { calls += "voice ptt $on" }
        override fun capturing(call: ULong) { calls += "voice capturing $call" }
        override fun focus(call: ULong, focus: AudioFocus) { calls += "voice focus $call $focus" }
        override fun headset(press: HeadsetPress) { calls += "voice headset $press" }
        override fun stats(call: ULong, request: ULong, stats: PlayoutStats) { calls += "voice stats $call $request" }
        override fun end() { calls += "voice end" }
        override fun dismissed(call: ULong) { calls += "voice dismissed $call" }
    }
}
