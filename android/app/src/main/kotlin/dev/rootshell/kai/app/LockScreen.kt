package dev.rootshell.kai.app

import dev.rootshell.kai.R

/**
 * Privacy by default on the lock screen (design §12.3, the owner's decision), whatever Android's own
 * lock-screen setting says: while the phone is locked every notification Kai posts carries only the
 * content-free text of its kind as its main content ("Kai needs your approval", "New reply from
 * Kai", "Kai · voice call" with Mute and End), and its content once the phone is unlocked. Android's
 * default setting ("Show all notification content") would show a private notification's content
 * on the lock screen, so `VISIBILITY_PRIVATE` with a public version is only the second line.
 *
 * The [Notifier] posts each notification again with the other face when the lock changes (the
 * screen going off, the owner present after unlocking).
 */
object LockScreen {
    /** The kinds of notification Kai posts. */
    enum class Kind { APPROVAL, REPLY, NOTICE, STATUS, CALL }

    /**
     * What a notification shows: its title and text, the expanded text, a sub-text (a reply's
     * count), and whether its actions are on it.
     */
    data class Face(
        val title: String,
        val text: String,
        val bigText: String? = null,
        val subText: String? = null,
        val actions: Boolean = true,
    )

    /** The content-free text of each kind. */
    fun contentFree(kind: Kind): Int = when (kind) {
        Kind.APPROVAL -> R.string.public_approval
        Kind.REPLY -> R.string.public_reply
        Kind.NOTICE -> R.string.public_notice
        Kind.STATUS -> R.string.public_status
        Kind.CALL -> R.string.public_call
    }

    /**
     * Only a call keeps its actions while locked (Mute or Unmute, Take over, End: controls that say
     * nothing); an approval's Approve and Decline need the call they decide, and Talk and Take over
     * open the app.
     */
    fun actionsWhileLocked(kind: Kind): Boolean = kind == Kind.CALL

    /** The face posted as the notification's main content; `words` reads a string resource. */
    fun face(kind: Kind, full: Face, locked: Boolean, words: (Int) -> String): Face =
        if (!locked) {
            full
        } else {
            Face(words(R.string.app_name), words(contentFree(kind)), actions = actionsWhileLocked(kind))
        }

    /**
     * Whether a notification posted now may be seen on the lock screen: the keyguard is locked, or
     * the screen is off (the keyguard may lock while it is off, after Android's lock delay, and no
     * broadcast says when; a screen that is off shows nothing, so the content-free face costs
     * nothing there).
     */
    fun locked(keyguardLocked: Boolean, interactive: Boolean): Boolean = keyguardLocked || !interactive
}
