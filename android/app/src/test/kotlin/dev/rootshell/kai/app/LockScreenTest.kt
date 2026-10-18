package dev.rootshell.kai.app

import dev.rootshell.kai.R
import dev.rootshell.kai.app.LockScreen.Face
import dev.rootshell.kai.app.LockScreen.Kind
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

class LockScreenTest {
    /** A string resource as its id, so the test sees which text was chosen. */
    private val words = { id: Int -> "res:$id" }

    private val content = Face(
        title = "Kai: Groceries",
        text = "lmgw__delete_files wants to run: rm -rf ~/receipts",
        bigText = "lmgw__delete_files wants to run: rm -rf ~/receipts",
        subText = "3 replies",
    )

    @Test
    fun lockedEveryKindShowsOnlyItsContentFreeText() {
        val expected = mapOf(
            Kind.APPROVAL to R.string.public_approval,
            Kind.REPLY to R.string.public_reply,
            Kind.NOTICE to R.string.public_notice,
            Kind.STATUS to R.string.public_status,
            Kind.CALL to R.string.public_call,
        )
        for (kind in Kind.entries) {
            val face = LockScreen.face(kind, content, locked = true, words)
            assertEquals(kind.name, "res:${R.string.app_name}", face.title)
            assertEquals(kind.name, "res:${expected.getValue(kind)}", face.text)
            assertNull(kind.name, face.bigText)
            assertNull(kind.name, face.subText)
            // Only a call keeps its controls (Mute, End); an approval's Approve needs what it decides.
            assertEquals(kind.name, kind == Kind.CALL, face.actions)
        }
    }

    @Test
    fun unlockedTheContentShowsWithItsActions() {
        for (kind in Kind.entries) {
            assertEquals(kind.name, content, LockScreen.face(kind, content, locked = false, words))
        }
    }

    @Test
    fun theLockIsTheKeyguardOrAScreenThatIsOff() {
        assertTrue(LockScreen.locked(keyguardLocked = true, interactive = true))
        // The keyguard may lock while the screen is off, after Android's lock delay, unannounced.
        assertTrue(LockScreen.locked(keyguardLocked = false, interactive = false))
        assertFalse(LockScreen.locked(keyguardLocked = false, interactive = true))
    }
}
