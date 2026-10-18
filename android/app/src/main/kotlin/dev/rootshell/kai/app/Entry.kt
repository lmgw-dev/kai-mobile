package dev.rootshell.kai.app

import android.content.Intent

/**
 * What an intent asks of [dev.rootshell.kai.MainActivity], which is exported (the launcher, a tapped
 * `lmgw-pair:` link, the assistant's filters), pure so the JVM tests run it (design §4.1, §9.1,
 * §9.4):
 * - `ASSIST` and `VOICE_COMMAND` (where a system or a headset sends them instead of showing the
 *   assistant's overlay) only open the voice screen with Talk ready, **never a call**: the filters
 *   are exported, so a headset's button in a pocket on a locked phone, or any foreground app's
 *   explicit intent, must not make Kai listen. A call starts by the owner's own tap on Talk or by
 *   the system's assistant gesture (the session, §9.2); `ASSIST` with the keyboard hint opens the
 *   conversation to type instead;
 * - a `lmgw-pair:` link only ever leads to the pairing's confirmation;
 * - anything else opens the app as it is.
 */
sealed interface Entry {
    data class Pair(val link: String) : Entry

    /** The voice screen, Talk ready: no call starts. */
    data object Voice : Entry

    data object Type : Entry

    data object Open : Entry

    companion object {
        fun of(action: String?, data: String?, keyboardHint: Boolean): Entry = when (action) {
            Intent.ACTION_ASSIST -> if (keyboardHint) Type else Voice
            Intent.ACTION_VOICE_COMMAND -> Voice
            else -> PairingLinks.of(data)?.let { Pair(it) } ?: Open
        }
    }
}
