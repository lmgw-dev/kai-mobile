package dev.rootshell.kai.app

import dev.rootshell.kai.core.ffi.CallPhase
import dev.rootshell.kai.core.ffi.CoreException
import dev.rootshell.kai.core.ffi.FolderChoice
import dev.rootshell.kai.core.ffi.OpenedStatus
import dev.rootshell.kai.core.ffi.OpenedView
import dev.rootshell.kai.core.ffi.SearchPageView
import dev.rootshell.kai.core.ffi.SearchThreadView
import dev.rootshell.kai.core.ffi.VoiceView
import dev.rootshell.kai.voice.active
import dev.rootshell.kai.voice.moving

/** What an opened thread says and offers about voice ([ThreadRules.voice], design §7.12). */
sealed interface ThreadVoice {
    /** No call runs: Talk binds this thread ([talk] false on a temporary or a gone thread). */
    data class Idle(val talk: Boolean) : ThreadVoice

    /** The call is bound to this thread; [leaving]: a Talk elsewhere moves it at its next quiet point. */
    data class Here(val leaving: Boolean) : ThreadVoice

    /**
     * The call is bound elsewhere: the Kai conversation ([threadId] null) or another thread, named by
     * [title] when lmgw named it; [coming]: it moves here at its next quiet point; [talk]: Talk here
     * (none on a temporary or a gone thread, nor while a move goes).
     */
    data class Elsewhere(val threadId: Long?, val title: String?, val coming: Boolean, val talk: Boolean) : ThreadVoice
}

/** A thread the owner opened from the browser, and the message a search hit named (design §6.4). */
data class ThreadTarget(val id: Long, val messageId: Long? = null)

/** What a thread's menu offers. */
enum class ThreadAction { PIN, UNPIN, ARCHIVE, RESTORE, MOVE, RENAME, DELETE }

/** One place a thread can be moved to: a folder, or out of any (`folderId` null). */
data class MoveTarget(val folderId: Long?, val name: String?)

/**
 * The browser's decisions (design §6.4), pure so the JVM tests run them; what lmgw allows and what
 * the lists hold are the core's.
 */
object ThreadRules {
    /** lmgw's minimum for a search (the core refuses below it as well, without a request). */
    const val SEARCH_MIN_CHARS = 2

    fun searchable(query: String): Boolean = query.trim().length >= SEARCH_MIN_CHARS

    /**
     * What opened thread [thread] says and offers about voice (design §7.12): Talk binds it while no
     * call runs (none on a temporary thread, rule 6, nor on a gone one); a call bound to it is
     * "Voice call in this thread", and one bound elsewhere names where (the Kai conversation,
     * `threadId` null, or another thread) with Talk here, which moves that call here at its next
     * quiet point. The thread that is the Kai conversation's now ([OpenedView.kai]) is where a call
     * bound by the folder talks.
     *
     * While a move goes (the old call ended, the restart it owes not yet a call: §7.7's gap) the
     * call still runs, bound to the restart's target: that thread says the call is here, and no
     * thread offers Talk, since a start then would race the restart.
     */
    fun voice(view: VoiceView?, thread: OpenedView): ThreadVoice {
        val talk = !thread.temporary && thread.status != OpenedStatus.GONE
        if (view == null || !view.active()) return ThreadVoice.Idle(talk = talk)
        val moving = view.moving()
        val bound = if (moving) view.end?.restart?.thread else view.bindThread
        val target = view.switching.takeUnless { moving }
        fun here(t: Long?) = t == thread.threadId || (t == null && thread.kai)
        if (here(bound)) return ThreadVoice.Here(leaving = target != null)
        return ThreadVoice.Elsewhere(
            threadId = bound,
            title = view.threadTitle?.takeIf { !moving && bound != null && it.isNotBlank() },
            coming = target != null && here(target.thread),
            talk = talk && !moving,
        )
    }

    /**
     * A send in this conversation cancels the call's voice turn (lmgw's rule, design §6.3, §7.12):
     * a call runs bound to it. `threadId` null is the Kai conversation, which a call bound by the
     * folder talks in, as it does in an opened thread that is the Kai conversation's now ([kai]).
     * Not while a move goes: neither call holds a turn then.
     */
    fun sendStopsVoice(view: VoiceView?, threadId: Long?, kai: Boolean = false): Boolean {
        if (view == null || !running(view.phase) || view.moving()) return false
        val bound = view.bindThread
        return bound == threadId || (bound == null && kai)
    }

    /** The phone's call runs, from the start to its end. */
    private fun running(phase: CallPhase): Boolean =
        phase == CallPhase.CONNECTING || phase == CallPhase.PENDING || phase == CallPhase.LIVE || phase == CallPhase.ENDING

    /** A thread's menu: none for a temporary thread (lmgw refuses each action with 409). */
    fun actions(pinned: Boolean, archived: Boolean, temporary: Boolean): List<ThreadAction> =
        if (temporary) {
            emptyList()
        } else {
            listOf(
                if (pinned) ThreadAction.UNPIN else ThreadAction.PIN,
                if (archived) ThreadAction.RESTORE else ThreadAction.ARCHIVE,
                ThreadAction.MOVE,
                ThreadAction.RENAME,
                ThreadAction.DELETE,
            )
        }

    /** Where a thread in `current` can go: every other folder the device sees, and "No folder". */
    fun moveTargets(folders: List<FolderChoice>, current: Long?): List<MoveTarget> =
        folders.filter { it.folderId != current }.map { MoveTarget(it.folderId, it.name) } +
            listOfNotNull(MoveTarget(null, null).takeIf { current != null })

    /** The next search page appended; a thread already shown (the results moved meanwhile) stays once. */
    fun append(shown: List<SearchThreadView>, page: SearchPageView): List<SearchThreadView> {
        val ids = shown.map { it.threadId }.toSet()
        return shown + page.threads.filter { it.threadId !in ids }
    }

    /** A refusal for the owner's eyes: lmgw's words with its code, and the hint the core adds. */
    fun words(e: CoreException): String = when (e) {
        is CoreException.Refused -> listOfNotNull("${e.reason} (${e.code})", e.hint).joinToString(": ")
        is CoreException.NotConnected -> e.reason
        is CoreException.NotReady -> e.reason
        is CoreException.Unreachable -> e.reason
        else -> e.message ?: e.javaClass.simpleName
    }
}
