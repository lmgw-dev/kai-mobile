package dev.rootshell.kai.voice

import dev.rootshell.kai.core.ffi.CallPhase
import dev.rootshell.kai.core.ffi.VoiceView

/**
 * Whether a call is under way: from the trigger to the PeerConnection's close, and on through a
 * rebind's or a move's gap ([moving]) until the next call is the core's.
 */
fun VoiceView?.active(): Boolean =
    this != null && (phase in setOf(CallPhase.CONNECTING, CallPhase.PENDING, CallPhase.LIVE, CallPhase.ENDING) || moving())

/**
 * A rebind or a move to another thread goes (design §7.7, §7.12): the call ended with the restart
 * it owes, which Kotlin starts once the old PeerConnection is closed. The core drops the restart
 * from the view when the link refused it, so this never outlives the gap.
 */
fun VoiceView?.moving(): Boolean =
    this != null && (phase == CallPhase.ENDING || phase == CallPhase.ENDED) && end?.restart != null

/** How a call's start went ([CallEngine.start]). */
sealed interface Started {
    /** The core took call [id] (its number); its end comes through the engine's listener. */
    data class Call(val id: ULong) : Started

    /** Refused at once (not paired, not connected, a call running, …), in the core's words. */
    data class Refused(val words: String) : Started

    /** A call runs bound elsewhere: it moves to this binding at its next quiet point (design §7.12). */
    data object Moving : Started
}
