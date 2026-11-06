package dev.rootshell.kai.app

import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateMapOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import dev.rootshell.kai.core.ffi.CoreException
import dev.rootshell.kai.core.ffi.SearchIn
import dev.rootshell.kai.core.ffi.SearchPageView
import dev.rootshell.kai.core.ffi.SearchThreadView
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Job
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch

/**
 * What the owner has typed and chosen in the threads browser, held by the hub (design §6.4): the
 * draft of each thread, the search, whether the archived section is open. The screens draw from it
 * and write to it, so a fold or an unfold (the list and the thread are then composed at other
 * places) and the compact window's list and thread pages lose nothing. It lasts for the process;
 * leaving the browser clears the search and the open section (the core drops its lists then), not
 * the drafts.
 */
class ThreadsUi(scope: CoroutineScope, search: suspend (String, SearchIn, Long?, Long?) -> SearchPageView) {
    private val drafts = mutableStateMapOf<Long, String>()
    val search = ThreadSearch(scope, search)

    /** The archived section is open (the core reads and keeps its list current then). */
    var archivedOpen by mutableStateOf(false)

    fun draft(thread: Long): String = drafts[thread].orEmpty()

    fun setDraft(thread: Long, text: String) {
        if (text.isEmpty()) drafts.remove(thread) else drafts[thread] = text
    }

    /** The browser was left. */
    fun left() {
        search.clear()
        archivedOpen = false
    }
}

/**
 * lmgw's full-text search as the browser shows it (design §6.4): from two characters on, a moment
 * after the last key; the scope (All by default) and a folder filter; More results by the page's
 * `next_offset`. A new query, scope or folder cancels the page being fetched, so a page never lands
 * on another query's results.
 */
class ThreadSearch(
    private val scope: CoroutineScope,
    private val fetch: suspend (String, SearchIn, Long?, Long?) -> SearchPageView,
    private val debounceMs: Long = 300,
) {
    var query by mutableStateOf("")
        private set
    var scopeIn by mutableStateOf(SearchIn.ALL)
        private set
    var folder by mutableStateOf<Long?>(null)
        private set
    var results by mutableStateOf<List<SearchThreadView>>(emptyList())
        private set
    var total by mutableStateOf<Long?>(null)
        private set
    var next by mutableStateOf<Long?>(null)
        private set
    var error by mutableStateOf<String?>(null)
        private set

    private var first: Job? = null
    private var more: Job? = null

    fun typeQuery(text: String) {
        query = text
        restart()
    }

    fun chooseScope(value: SearchIn) {
        scopeIn = value
        restart()
    }

    fun chooseFolder(id: Long?) {
        folder = id
        restart()
    }

    /** Back to nothing typed, the defaults chosen. */
    fun clear() {
        query = ""
        scopeIn = SearchIn.ALL
        folder = null
        restart()
    }

    private fun restart() {
        first?.cancel()
        more?.cancel()
        results = emptyList()
        total = null
        next = null
        error = null
        if (!ThreadRules.searchable(query)) return
        val (q, s, f) = Triple(query, scopeIn, folder)
        first = scope.launch {
            delay(debounceMs)
            try {
                val page = fetch(q, s, f, null)
                results = page.threads
                total = page.total
                next = page.nextOffset
            } catch (e: CoreException) {
                error = ThreadRules.words(e)
            }
        }
    }

    /** The next page, appended to the results it continues. */
    fun more() {
        val offset = next ?: return
        if (more?.isActive == true) return
        val (q, s, f) = Triple(query, scopeIn, folder)
        more = scope.launch {
            try {
                val page = fetch(q, s, f, offset)
                results = ThreadRules.append(results, page)
                next = page.nextOffset
            } catch (e: CoreException) {
                error = ThreadRules.words(e)
            }
        }
    }
}
