package dev.rootshell.kai.voice

import dev.rootshell.kai.core.ffi.CallEnd
import dev.rootshell.kai.core.ffi.CallStart
import dev.rootshell.kai.core.ffi.EndReason
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

/** The order the engine reports a call's start and end in (design §7.3). */
class CallOrderTest {
    private fun end(reason: EndReason, restart: CallStart? = null) = CallEnd(reason, null, null, null, emptyList(), restart)

    @Test
    fun endArrivesBeforeStartIsRecorded() {
        val order = CallOrder()
        val n = order.asked()
        // The core ended the call before the voice thread recorded its start.
        val e = end(EndReason.FAILED)
        assertNull(order.ended(n, e))
        assertEquals(0, order.current)
        // The start, then the end: the audio is set up and released, never left held.
        assertEquals(listOf(CallOrder.Heard.Starting(7uL), CallOrder.Heard.Ended(e)), order.took(n, 7uL))
        assertEquals(n, order.current)
    }

    @Test
    fun aCallsEndIsHeardOnceItStartedAndAnOlderCallsNot() {
        val order = CallOrder()
        val first = order.asked()
        assertEquals(listOf(CallOrder.Heard.Starting(1uL)), order.took(first, 1uL))
        assertTrue(order.cue(first))
        val e = end(EndReason.ENDED)
        assertEquals(CallOrder.Heard.Ended(e), order.ended(first, e))
        val second = order.asked()
        order.took(second, 2uL)
        assertNull("the older call's", order.ended(first, e))
        assertFalse(order.cue(first))
        // A rebind's end keeps the audio for the next call.
        val again = CallStart(new = false, takeOver = false)
        assertEquals(CallOrder.Heard.Rebind(again), order.ended(second, end(EndReason.REBIND, again)))
    }

    @Test
    fun aRefusedStartForgetsAnEndThatCameFirst() {
        val order = CallOrder()
        val n = order.asked()
        order.ended(n, end(EndReason.FAILED))
        order.refused(n)
        val next = order.asked()
        assertEquals(listOf(CallOrder.Heard.Starting(2uL)), order.took(next, 2uL))
    }
}
