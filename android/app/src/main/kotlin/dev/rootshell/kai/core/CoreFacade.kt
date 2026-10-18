package dev.rootshell.kai.core

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

/**
 * The Rust core as the app sees it. The generated `Core` object sits behind this interface so view
 * models, the hub and the screens are tested against a fake without loading the native library
 * (design §15.3); the records it passes (`Update`, `Settings`, …) are the bindings' plain data
 * classes. Every protocol rule is the core's (ruling 2): this is the boundary, nothing more.
 */
interface CoreFacade {
    fun version(): String
    fun lmgwPin(): LmgwPin
    fun build(): CoreBuild
    fun colours(): Colours

    /** Why the settings file was set aside at start, if it was. */
    fun settingsNotice(): String?

    fun settings(): Settings
    suspend fun setSettings(settings: Settings)

    /** Whether a pairing is stored (the boot receiver starts Kai's service by it). */
    fun paired(): Boolean

    /** The link parsed (strict v2) and what the owner confirms; no network (design §4.1). */
    suspend fun pairingPreview(text: String): PairingPreview

    /** Enrolment and the confirmation's start (design §4.2); the text is dropped afterwards. */
    suspend fun pair(text: String): PairingView

    /** Forget gateway (design §3.3). */
    suspend fun forget()

    /** Check now / Try again. */
    fun retry()
    fun setNetwork(net: NetworkFacts)
    fun setForeground(visible: Boolean)

    /**
     * The assistant overlay shows the approval cards (it shows, unlocked) or no longer does (design
     * §9.2, §11): with the app's visibility it tells the core whether a card is seen, so no
     * notification asks for the same call.
     */
    fun setOverlay(showingCards: Boolean)

    /** The alarm fired: compare the deadlines with the boot clock (design §12.2). */
    fun wake()

    /** The next update; one collector reads them for the process's life. */
    suspend fun nextUpdate(): Update
    suspend fun sendText(text: String)
    suspend fun newConversation()
    suspend fun decide(approvalRequestId: String, approve: Boolean): Decided
    /** Returns once `approvals.json` holds the dismissal; throws when it could not be written. */
    suspend fun dismissApproval(approvalRequestId: String)
    fun notificationGone(id: NotificationId)

    /** The voice entry points (design §3.2, §7). */
    val voice: VoiceFacade
}

/**
 * The core's `Voice` (design §3.2): the call's signalling and its events through the core. The
 * call's rules are the core's; Kotlin's [dev.rootshell.kai.voice.CallEngine] owns libwebrtc and
 * carries out what the core decides through the [EventSink] it hands to [start].
 */
interface VoiceFacade {
    /** A warm connection to lmgw while the offer is built (§7.2). */
    fun prepare()

    /**
     * The trigger (§7.3 step 1); throws with the state's words when voice cannot start, else answers
     * the call's number, which every libwebrtc event of the call carries back (the core drops an
     * older call's events: its PeerConnection may still be closing).
     */
    suspend fun start(start: CallStart, sink: EventSink): ULong

    /** The offer POSTed; lmgw's answer SDP. Throws when the call could not be set up (it has ended). */
    suspend fun offer(call: ULong, sdp: String): String
    fun transport(call: ULong, state: TransportState, detail: String)
    fun channelOpen(call: ULong)
    fun channelMessage(call: ULong, text: String)
    fun channelClosed(call: ULong, why: String)

    /** The phone cannot carry call `call` on (libwebrtc failed, Android refused the microphone type): it ends with `words`. */
    fun failed(call: ULong, words: String)
    fun takeOver()
    fun press()
    fun release()
    fun tap()
    fun stop()
    fun setMuted(muted: Boolean)
    fun setPushToTalk(on: Boolean)
    fun capturing(call: ULong)

    /** Call `call`'s audio focus changed (§8.5). */
    fun focus(call: ULong, focus: AudioFocus)

    /** A headset button during the call (§8.4). */
    fun headset(press: HeadsetPress)
    fun stats(call: ULong, request: ULong, stats: PlayoutStats)
    fun end()

    /**
     * The assistant overlay that started call `call` was dismissed (§9.2): the core ends it before
     * the owner's first turn and leaves it going on after.
     */
    fun dismissed(call: ULong)
}

data class LmgwPin(val commit: String, val tag: String, val tree: String)

data class CoreBuild(val profile: String, val target: String)

enum class Colours { Kai, System }
