package dev.rootshell.kai.app

import org.junit.Assert.assertEquals
import org.junit.Test

/** What MainActivity's exported filters lead to (design §4.1, §9.1, §9.4). */
class EntryTest {
    @Test
    fun assistAndVoiceCommandOpenTheVoiceScreenWithoutACallTheKeyboardHintTypes() {
        // Exported filters: a headset's button on a locked phone or an app's explicit ASSIST never
        // starts a call (Entry has no Talk); the owner's tap on Talk does.
        assertEquals(Entry.Voice, Entry.of("android.intent.action.ASSIST", null, keyboardHint = false))
        assertEquals(Entry.Type, Entry.of("android.intent.action.ASSIST", null, keyboardHint = true))
        assertEquals(Entry.Voice, Entry.of("android.intent.action.VOICE_COMMAND", null, keyboardHint = false))
    }

    @Test
    fun aPairingLinkOnlyLeadsToTheConfirmationAndAnythingElseOpens() {
        val link = "lmgw-pair:?v=2&name=desktop-a&url=https%3A%2F%2F192.0.2.10%3A8788&fp=sha256:00&token=lmgw-enrol-x"
        assertEquals(Entry.Pair(link), Entry.of("android.intent.action.VIEW", link, keyboardHint = false))
        // An assistant intent never pairs, whatever data it carries.
        assertEquals(Entry.Voice, Entry.of("android.intent.action.ASSIST", link, keyboardHint = false))
        assertEquals(Entry.Open, Entry.of("android.intent.action.MAIN", null, keyboardHint = false))
        assertEquals(Entry.Open, Entry.of("android.intent.action.VIEW", "https://desktop-a.example/", keyboardHint = false))
        assertEquals(Entry.Open, Entry.of(null, null, keyboardHint = false))
    }
}
