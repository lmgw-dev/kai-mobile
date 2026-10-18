package dev.rootshell.kai.voice

import dev.rootshell.kai.core.ffi.CallPhase
import dev.rootshell.kai.core.ffi.VoiceView

/** Whether a call is under way (from the trigger to the PeerConnection's close). */
fun VoiceView?.active(): Boolean =
    this != null && phase in setOf(CallPhase.CONNECTING, CallPhase.PENDING, CallPhase.LIVE, CallPhase.ENDING)

/** How a call's start went ([CallEngine.start]). */
sealed interface Started {
    /** The core took call [id] (its number); its end comes through the engine's listener. */
    data class Call(val id: ULong) : Started

    /** Refused at once (not paired, not connected, a call running, …), in the core's words. */
    data class Refused(val words: String) : Started
}
