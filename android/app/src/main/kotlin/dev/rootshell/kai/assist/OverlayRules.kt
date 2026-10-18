package dev.rootshell.kai.assist

import dev.rootshell.kai.R
import dev.rootshell.kai.core.ffi.CallEnd
import dev.rootshell.kai.core.ffi.CallPhase
import dev.rootshell.kai.core.ffi.CallStart
import dev.rootshell.kai.core.ffi.VoiceView
import dev.rootshell.kai.voice.Started
import dev.rootshell.kai.voice.active

/**
 * The assistant overlay's decisions (design §9.2, §9.6, §7.7), pure so the JVM tests run them;
 * [KaiSession] carries them out on the main thread. One instance lives in the application, so a
 * session the system destroyed and made again still knows which call the overlay started.
 *
 * - **A show** starts a call by the Kai folder at once, unless a call runs (it shows that one,
 *   starting nothing), the phone is locked (nothing starts and nothing of the conversation shows,
 *   §9.6), or the microphone is not allowed (the overlay cannot ask: Open Kai asks).
 * - **A hide** that is not Expand's is a dismissal: the core hears of it for the call this overlay
 *   started, and decides (before the owner's first turn the call ends, after it it goes on). A call
 *   the app started is never ended by the overlay: the invocation did not open its microphone.
 * - **Expand** (and Keyboard) hand the call to the app without a reconnect: the voice screen (or the
 *   conversation) of the running process shows the same call, and no new one is asked for. From
 *   then on the call is the app's, as one the app started.
 */
class OverlayModel {
    /** The core's number of the call this overlay started (null: none, or the app's call). */
    var started: ULong? = null
        private set

    /** Why this show started no call (null: it did, or one runs). */
    var blocked: Blocked? = null
        private set

    @Volatile
    var showing = false
        private set

    /** The hide in progress is Expand's (or Keyboard's, or Open Kai's): no dismissal. */
    private var handingOver = false

    /** A start the overlay asked for has not been answered yet. */
    var starting = false
        private set

    /** The overlay was dismissed while its start was unanswered: the call is dismissed at the answer. */
    private var dismissedWhileStarting = false

    /** The overlay handed over while its start was unanswered: the call is the app's at the answer. */
    private var handedOverWhileStarting = false

    /** What a show does now. */
    fun shown(facts: ShowFacts): Show {
        showing = true
        handingOver = false
        dismissedWhileStarting = false
        return decide(facts, CallStart(new = false, takeOver = false))
    }

    /** The phone was unlocked while the overlay showed its locked face: the invocation goes on. */
    fun unlocked(facts: ShowFacts): Show? =
        if (showing && blocked == Blocked.Locked) decide(facts, CallStart(new = false, takeOver = false)) else null

    /** Talk, Reconnect or Take over in the overlay after a call ended: as a show, with `start`. */
    fun talk(facts: ShowFacts, start: CallStart): Show = decide(facts, start)

    /**
     * The start's answer: the call the overlay now owns, or the core's words why not. Returns the
     * call to dismiss when the overlay was dismissed while the start was on its way.
     */
    fun answered(result: Started): ULong? {
        starting = false
        val dismiss = dismissedWhileStarting
        val handedOver = handedOverWhileStarting
        dismissedWhileStarting = false
        handedOverWhileStarting = false
        return when (result) {
            is Started.Call -> {
                // A call handed over to the app is the app's: the overlay never dismisses it.
                started = result.id.takeUnless { handedOver }
                blocked = null
                result.id.takeIf { dismiss }
            }
            is Started.Refused -> {
                blocked = Blocked.Words(result.words)
                null
            }
        }
    }

    /**
     * Expand, Keyboard, Unlock or Open Kai, **after** a path opened the app ([expandThrough]): the
     * hide that follows hands the call over (§9.2). From then on the call is the app's, so a later
     * show of the overlay does not end it at a dismissal. A handover that no path carried out never
     * reaches here: the overlay keeps its call and its sheet.
     */
    fun handover() {
        handingOver = true
        started = null
        if (starting) handedOverWhileStarting = true
    }

    /**
     * The overlay is hidden: the call whose dismissal the core hears, or null when nothing is to be
     * dismissed (a handover, or no call of the overlay's). It names the overlay's own call whatever
     * the call's state is here: the core ignores an ended call and any call but the current one, and
     * its view of the call is never a moment behind the overlay's (a commit in flight is never
     * raced). A start still on its way is dismissed at its answer too ([answered]).
     */
    fun hidden(): ULong? {
        if (!showing) return null
        val handover = handingOver
        showing = false
        handingOver = false
        if (handover) return null
        if (starting) dismissedWhileStarting = true
        return started
    }

    private fun decide(f: ShowFacts, start: CallStart): Show {
        blocked = null
        return when {
            f.locked -> {
                blocked = Blocked.Locked
                Show.Locked
            }
            // A call runs, or the overlay's start is on its way: shown, nothing more starts.
            f.call.active() || starting -> Show.Running
            !f.microphone -> {
                blocked = Blocked.Microphone
                Show.NeedsMicrophone
            }
            else -> {
                starting = true
                Show.Start(start)
            }
        }
    }
}

/** What a show finds. */
data class ShowFacts(
    /** The keyguard is locked (or the screen is off). */
    val locked: Boolean,
    /** `RECORD_AUDIO` is granted. */
    val microphone: Boolean,
    /** The call now (null before the core published one). */
    val call: VoiceView?,
)

/** What a show does. */
sealed interface Show {
    /** Locked: nothing starts, nothing of the conversation shows (§9.6). */
    data object Locked : Show

    /** A call runs: it is shown, nothing starts (§9.2: `onShow` can repeat). */
    data object Running : Show

    /** The microphone is not allowed: Open Kai asks for it (the overlay cannot). */
    data object NeedsMicrophone : Show

    /** A call by the Kai folder, at once (§7.3). */
    data class Start(val start: CallStart) : Show
}

/** Why a show started no call. */
sealed interface Blocked {
    data object Locked : Blocked
    data object Microphone : Blocked

    /** The core's words: not paired, not connected, the folder's steps, … */
    data class Words(val words: String) : Blocked
}

/** Where a handover goes. */
enum class Into { VOICE, CONVERSATION }

/** How Expand opens the app (design §9.2). */
enum class ExpandPath {
    /** `VoiceInteractionSession.startAssistantActivity`. */
    ASSISTANT_ACTIVITY,

    /** `Context.startActivity` with `FLAG_ACTIVITY_NEW_TASK`. */
    START_ACTIVITY,
}

/**
 * **WP0 (a)'s seam for Expand:** the paths in the order they are tried, the next one when one
 * throws or the app does not show. `startActivity` first (one Kai in Recents, design item 23: the
 * session's own task would make a second), `startAssistantActivity` the fallback. WP0 (a) runs both
 * on a One UI and a LineageOS test device; its verdict reorders or drops one here.
 */
val EXPAND_PATHS: List<ExpandPath> = listOf(ExpandPath.START_ACTIVITY, ExpandPath.ASSISTANT_ACTIVITY)

/**
 * The first of [paths] whose attempt opened the app, or null when none did (the sheet then stays,
 * with its error, and the overlay keeps the call). An attempt is false when its start threw or the
 * app did not show within `lmgw.timeout_s`: Android 14 and later refuse a start from the background
 * without throwing, so the app showing is the only signal.
 */
suspend fun expandThrough(paths: List<ExpandPath>, attempt: suspend (ExpandPath) -> Boolean): ExpandPath? =
    paths.firstOrNull { attempt(it) }

/** What the lock watch does with a broadcast while the overlay shows (§9.6). */
sealed interface LockStep {
    /** The screen went off: the overlay is hidden, so the core's dismissal rule decides. */
    data object Hide : LockStep

    /** The lock state may have changed: redraw, and let an unlock continue the invocation. */
    data object Sync : LockStep
}

fun lockStep(action: String?): LockStep =
    if (action == android.content.Intent.ACTION_SCREEN_OFF) LockStep.Hide else LockStep.Sync

/** What the sheet shows. */
sealed interface Face {
    /**
     * The phone is locked: Kai's name and "Unlock to talk to Kai" only, and a running call's Mute
     * and End as the lock screen's notification has them (§12.3), never its words or captions.
     */
    data class Locked(val call: Boolean) : Face

    /** No call started: why, with Open Kai. */
    data class Blocked(val why: dev.rootshell.kai.assist.Blocked) : Face

    /** A call: the ring, the state line, the last captions, the controls. */
    data object Call : Face

    /** The last call ended while the overlay showed: its words, Talk or what it offers. */
    data class Ended(val end: CallEnd) : Face

    /** Between the show and the core's first view of the new call. */
    data object Starting : Face
}

/**
 * The sheet's face for the overlay's state now (`starting`: its start is on its way, so an older
 * call's end is not shown meanwhile).
 */
fun face(locked: Boolean, blocked: Blocked?, starting: Boolean, view: VoiceView?): Face {
    val end = view?.end
    return when {
        locked -> Face.Locked(call = view.active())
        view.active() -> Face.Call
        blocked != null -> Face.Blocked(blocked)
        starting -> Face.Starting
        view?.phase == CallPhase.ENDED && end != null -> Face.Ended(end)
        else -> Face.Starting
    }
}

/**
 * What closing the overlay does to the call, said under the controls (§9.2), so the owner knows
 * before the swipe: the overlay's own call before the owner's first turn ends, after it (and a call
 * the app started) goes on in the background. Null while no call runs.
 */
fun dismissalHint(view: VoiceView?, started: ULong?): Int? {
    if (view == null || !view.active()) return null
    val own = started == view.call
    return when {
        view.phase == CallPhase.PENDING && own -> R.string.overlay_hint_declines
        own && !view.spoken -> R.string.overlay_hint_ends
        else -> R.string.overlay_hint_goes_on
    }
}
