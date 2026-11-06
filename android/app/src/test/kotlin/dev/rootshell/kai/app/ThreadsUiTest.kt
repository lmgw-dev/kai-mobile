package dev.rootshell.kai.app

import dev.rootshell.kai.core.ffi.CoreException
import dev.rootshell.kai.core.ffi.SearchIn
import dev.rootshell.kai.core.ffi.SearchPageView
import dev.rootshell.kai.core.ffi.SearchThreadView
import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

private fun hit(id: Long) = SearchThreadView(id, "Thread $id", null, false, false, 1, emptyList())

class ThreadsUiTest {
    private val scope = CoroutineScope(SupervisorJob() + Dispatchers.Unconfined)

    private fun search(fetch: suspend (String, SearchIn, Long?, Long?) -> SearchPageView) = ThreadSearch(scope, fetch, debounceMs = 0)

    @Test
    fun draftsAreKeptPerThreadAndAnEmptyOneIsNone() {
        val ui = ThreadsUi(scope) { _, _, _, _ -> SearchPageView(0, emptyList(), null) }
        ui.setDraft(5, "soup?")
        ui.setDraft(6, "tea")
        assertEquals("soup?", ui.draft(5))
        assertEquals("tea", ui.draft(6))
        ui.setDraft(5, "")
        assertEquals("", ui.draft(5))
        // Leaving the browser clears the search and the open section, not what was typed to a thread.
        ui.archivedOpen = true
        ui.search.typeQuery("a")
        ui.left()
        assertEquals("", ui.search.query)
        assertTrue(!ui.archivedOpen)
        assertEquals("tea", ui.draft(6))
    }

    @Test
    fun aSearchRunsFromTwoCharactersAndKeepsItsChoices() {
        val calls = mutableListOf<String>()
        val s = search { q, scopeIn, folder, offset ->
            calls += "$q $scopeIn $folder $offset"
            SearchPageView(2, listOf(hit(1), hit(2)), 20)
        }
        s.typeQuery("a")
        assertTrue(calls.isEmpty())
        s.typeQuery("ab")
        s.chooseFolder(3)
        assertEquals(listOf("ab ALL null null", "ab ALL 3 null"), calls)
        assertEquals(listOf(1L, 2L), s.results.map { it.threadId })
        assertEquals(20L, s.next)
        s.typeQuery("")
        assertTrue(s.results.isEmpty())
        assertNull(s.total)
    }

    @Test
    fun aMorePageNeverLandsOnAnotherQuery() = runBlocking {
        val gate = CompletableDeferred<Unit>()
        val s = search { q, _, _, offset ->
            if (offset == null) {
                SearchPageView(4, listOf(hit(1)), 20)
            } else {
                gate.await()
                SearchPageView(4, listOf(hit(9)), null)
            }
        }
        s.typeQuery("soup")
        s.more()
        // The query changes while the page is fetched: its answer is dropped.
        s.typeQuery("tea")
        gate.complete(Unit)
        assertEquals(listOf(1L), s.results.map { it.threadId })
        assertEquals(20L, s.next)
        // The next page of the new query appends.
        s.more()
        assertEquals(listOf(1L, 9L), s.results.map { it.threadId })
        assertNull(s.next)
    }

    @Test
    fun aRefusalIsShownInPlaceOfTheResults() {
        val s = search { _, _, _, _ -> throw CoreException.NotConnected("not connected") }
        s.typeQuery("soup")
        assertEquals("not connected", s.error)
        s.typeQuery("soups")
        assertEquals("not connected", s.error)
        s.typeQuery("")
        assertNull(s.error)
    }
}
