package dev.rootshell.kai.ui.voice

import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

/** The ring's halo (design §10.3): it writes, and so draws, only while it moves. */
class RingTest {
    @Test
    fun theHaloWritesOnlyWhileItMoves() {
        // Silence, and any steady level: nothing to write, nothing drawn.
        assertNull(nextLevel(0f, 0f))
        assertNull(nextLevel(0.4f, 0.402f))
        // Up fast, down slowly.
        val up = nextLevel(0f, 1f)!!
        val down = nextLevel(1f, 0f)!!
        assertTrue(up > 1f - down)
        // The last step lands on the target, so the writes stop.
        var level = 1f
        var steps = 0
        while (true) {
            level = nextLevel(level, 0f) ?: break
            steps++
            assertTrue("settles", steps < 200)
        }
        assertEquals(0f, level, 0f)
        assertNotNull(nextLevel(0f, 0.5f))
    }
}
