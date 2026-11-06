package dev.rootshell.kai.ui.threads

import androidx.compose.foundation.clickable
import androidx.compose.foundation.combinedClickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.LazyListScope
import androidx.compose.foundation.lazy.items
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.KeyboardArrowRight
import androidx.compose.material.icons.filled.Clear
import androidx.compose.material.icons.filled.KeyboardArrowDown
import androidx.compose.material.icons.filled.Search
import androidx.compose.material.icons.filled.Star
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.DropdownMenu
import androidx.compose.material3.DropdownMenuItem
import androidx.compose.material3.FilterChip
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.ListItem
import androidx.compose.material3.ListItemDefaults
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.pluralStringResource
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.AnnotatedString
import androidx.compose.ui.text.SpanStyle
import androidx.compose.ui.text.buildAnnotatedString
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.text.withStyle
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import dev.rootshell.kai.KaiApplication
import dev.rootshell.kai.R
import dev.rootshell.kai.app.ThreadRules
import dev.rootshell.kai.app.ThreadSearch
import dev.rootshell.kai.core.ffi.BrowserStatus
import dev.rootshell.kai.core.ffi.BrowserView
import dev.rootshell.kai.core.ffi.FolderSection
import dev.rootshell.kai.core.ffi.SearchIn
import dev.rootshell.kai.core.ffi.SearchThreadView
import dev.rootshell.kai.core.ffi.SnippetPart
import dev.rootshell.kai.core.ffi.ThreadItem

/** What the list hands up: a thread to open (a search hit names its message), the Kai conversation. */
class BrowserActions(
    val open: (threadId: Long, kai: Boolean, messageId: Long?) -> Unit,
    val newIn: (FolderSection) -> Unit,
    val onError: (String) -> Unit,
)

/**
 * The browser's list (design §6.4): the search field, then either the search's results (scope,
 * folder, More results) or the folders with their threads (pinned first, as lmgw lists them), the
 * threads in no folder, the temporary ones and the archived section. Each row opens its thread, and
 * its menu (overflow button, long press) holds the thread's actions.
 */
@Composable
fun BrowserList(app: KaiApplication, selected: Long?, actions: BrowserActions, modifier: Modifier = Modifier) {
    val browser by app.hub.browser.collectAsStateWithLifecycle()
    // The query, the search's choices and the open section are the hub's, so a fold (which moves
    // this list to another place in the composition) and the compact window's two pages lose none.
    val ui = app.hub.threadsUi
    val search = ui.search
    val query = search.query
    val archivedOpen = ui.archivedOpen
    LaunchedEffect(archivedOpen) { app.core.browserArchived(archivedOpen) }
    Column(modifier) {
        OutlinedTextField(
            value = query,
            onValueChange = { search.typeQuery(it) },
            placeholder = { Text(stringResource(R.string.threads_search)) },
            leadingIcon = { Icon(Icons.Default.Search, contentDescription = null) },
            trailingIcon = {
                if (query.isNotEmpty()) {
                    IconButton(onClick = { search.typeQuery("") }) { Icon(Icons.Default.Clear, contentDescription = stringResource(R.string.threads_search_clear)) }
                }
            },
            singleLine = true,
            modifier = Modifier.fillMaxWidth().padding(horizontal = 12.dp, vertical = 4.dp),
        )
        val view = browser
        if (query.isNotBlank()) {
            SearchResults(search, view, selected, actions, Modifier.weight(1f))
        } else {
            LazyColumn(Modifier.weight(1f).fillMaxWidth()) {
                when {
                    view == null || (view.status != BrowserStatus.READY && view.sections.isEmpty()) -> item {
                        Note(view?.note ?: stringResource(R.string.threads_loading))
                    }
                    else -> {
                        view.note?.let { item { Note(it) } }
                        sections(app, view, selected, actions)
                        if (view.temporary.isNotEmpty()) {
                            item { SectionHeader(stringResource(R.string.threads_temporary)) }
                            items(view.temporary, key = { "t:${it.id}" }) { ThreadRow(app, it, view, selected, actions) }
                        }
                        if (view.archivedCount > 0) {
                            item {
                                ListItem(
                                    headlineContent = { Text(stringResource(R.string.threads_archived, view.archivedCount)) },
                                    leadingContent = {
                                        Icon(if (archivedOpen) Icons.Default.KeyboardArrowDown else Icons.AutoMirrored.Filled.KeyboardArrowRight, contentDescription = null)
                                    },
                                    modifier = Modifier.clickable { ui.archivedOpen = !archivedOpen },
                                )
                            }
                            if (archivedOpen) {
                                val archived = view.archived
                                if (archived == null) {
                                    item { Note(stringResource(R.string.threads_loading)) }
                                } else {
                                    items(archived, key = { "a:${it.id}" }) { ThreadRow(app, it, view, selected, actions) }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

private fun LazyListScope.sections(app: KaiApplication, view: BrowserView, selected: Long?, actions: BrowserActions) {
    view.sections.forEach { section ->
        item(key = "f:${section.folderId}") {
            Row(Modifier.fillMaxWidth().padding(start = 16.dp, end = 8.dp, top = 12.dp), verticalAlignment = Alignment.CenterVertically) {
                Text(
                    section.name.ifBlank { stringResource(R.string.threads_no_folder) },
                    style = MaterialTheme.typography.titleSmall,
                    color = MaterialTheme.colorScheme.primary,
                    modifier = Modifier.weight(1f),
                )
                // New thread where the folder names a model (New conversation in the Kai folder);
                // elsewhere the same button says what to do on the dashboard. None outside a folder.
                if (section.folderId != null) {
                    TextButton(onClick = { actions.newIn(section) }) {
                        Text(stringResource(if (section.kai) R.string.threads_new_in_kai else R.string.threads_new))
                    }
                }
            }
        }
        if (section.threads.isEmpty() && section.folderId != null) {
            item(key = "e:${section.folderId}") { Note(stringResource(R.string.threads_empty_folder)) }
        }
        items(section.threads, key = { "r:${it.id}" }) { ThreadRow(app, it, view, selected, actions) }
    }
}

@Composable
private fun SectionHeader(text: String) {
    Text(text, style = MaterialTheme.typography.titleSmall, color = MaterialTheme.colorScheme.primary, modifier = Modifier.padding(start = 16.dp, top = 16.dp, bottom = 4.dp))
}

@Composable
private fun Note(text: String) {
    Text(text, style = MaterialTheme.typography.bodyMedium, color = MaterialTheme.colorScheme.onSurfaceVariant, modifier = Modifier.padding(horizontal = 16.dp, vertical = 8.dp))
}

@Composable
private fun ThreadRow(app: KaiApplication, item: ThreadItem, view: BrowserView, selected: Long?, actions: BrowserActions) {
    var menu by remember { mutableStateOf(false) }
    val marks = buildList {
        if (item.kai) add(stringResource(R.string.threads_kai_mark))
        if (item.pinned) add(stringResource(R.string.threads_pinned_mark))
        if (item.archived) add(stringResource(R.string.threads_archived_mark))
        if (item.day.isNotEmpty()) add(item.day)
    }
    val thread = MenuThread(item.id, item.title, item.folderId, item.pinned, item.archived, item.temporary)
    Box {
        ListItem(
            headlineContent = { Text(item.title.ifBlank { stringResource(R.string.threads_untitled) }, maxLines = 1, overflow = TextOverflow.Ellipsis) },
            supportingContent = if (marks.isEmpty()) null else ({ Text(marks.joinToString(" · ")) }),
            leadingContent = if (item.pinned) ({ Icon(Icons.Default.Star, contentDescription = null) }) else null,
            trailingContent = { ThreadMenuButton(app, thread, view.folders, actions.onError) },
            colors = ListItemDefaults.colors(
                containerColor = if (item.id == selected) MaterialTheme.colorScheme.primaryContainer else MaterialTheme.colorScheme.surface,
            ),
            modifier = Modifier.combinedClickable(
                onClick = { actions.open(item.id, item.kai, null) },
                onLongClick = { if (ThreadRules.actions(item.pinned, item.archived, item.temporary).isNotEmpty()) menu = true },
                onLongClickLabel = stringResource(R.string.threads_menu),
            ),
        )
        ThreadMenu(app, thread, view.folders, menu, { menu = false }, actions.onError)
    }
}

/**
 * lmgw's full-text search (design §6.4): from two characters on, a moment after the last key; the
 * scope (Active, Archived, All; All by default) and a folder filter; More results by the page's
 * `next_offset`. Each hit opens its thread at the matching message.
 */
@Composable
private fun SearchResults(search: ThreadSearch, view: BrowserView?, selected: Long?, actions: BrowserActions, modifier: Modifier) {
    val query = search.query
    val searchable = ThreadRules.searchable(query)
    val error = search.error
    val total = search.total
    LazyColumn(modifier.fillMaxWidth()) {
        item {
            FlowRow(Modifier.padding(horizontal = 12.dp), horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                listOf(SearchIn.ACTIVE to R.string.threads_scope_active, SearchIn.ARCHIVED to R.string.threads_scope_archived, SearchIn.ALL to R.string.threads_scope_all)
                    .forEach { (s, label) -> FilterChip(selected = search.scopeIn == s, onClick = { search.chooseScope(s) }, label = { Text(stringResource(label)) }) }
                FolderFilter(view, search.folder) { search.chooseFolder(it) }
            }
        }
        when {
            !searchable -> item {
                Note(pluralStringResource(R.plurals.threads_search_short, ThreadRules.SEARCH_MIN_CHARS, ThreadRules.SEARCH_MIN_CHARS))
            }
            error != null -> item { Note(error) }
            total == 0L -> item { Note(stringResource(R.string.threads_search_none)) }
            total != null -> item {
                val n = total.toInt()
                Note(pluralStringResource(R.plurals.threads_search_total, n, n))
            }
        }
        items(search.results, key = { "s:${it.threadId}" }) { hit -> SearchRow(hit, view, selected, actions) }
        if (search.next != null) {
            item {
                TextButton(onClick = { search.more() }, modifier = Modifier.padding(horizontal = 8.dp)) {
                    Text(stringResource(R.string.threads_search_more))
                }
            }
        }
    }
}

@Composable
private fun FolderFilter(view: BrowserView?, folder: Long?, onPick: (Long?) -> Unit) {
    var open by remember { mutableStateOf(false) }
    val folders = view?.folders.orEmpty()
    Box {
        FilterChip(
            selected = folder != null,
            onClick = { open = true },
            label = { Text(folders.firstOrNull { it.folderId == folder }?.name ?: stringResource(R.string.threads_folder_any)) },
            trailingIcon = { Icon(Icons.Default.KeyboardArrowDown, contentDescription = null) },
        )
        DropdownMenu(expanded = open, onDismissRequest = { open = false }) {
            DropdownMenuItem(text = { Text(stringResource(R.string.threads_folder_any)) }, onClick = { open = false; onPick(null) })
            folders.forEach { f -> DropdownMenuItem(text = { Text(f.name) }, onClick = { open = false; onPick(f.folderId) }) }
        }
    }
}

@Composable
private fun SearchRow(hit: SearchThreadView, view: BrowserView?, selected: Long?, actions: BrowserActions) {
    val folder = view?.folders?.firstOrNull { it.folderId == hit.folderId }?.name
    val marks = listOfNotNull(
        if (hit.kai) stringResource(R.string.threads_kai_mark) else null,
        folder,
        if (hit.archived) stringResource(R.string.threads_archived_mark) else null,
    )
    ListItem(
        headlineContent = { Text(hit.title.ifBlank { stringResource(R.string.threads_untitled) }, maxLines = 1, overflow = TextOverflow.Ellipsis) },
        supportingContent = {
            Column {
                if (marks.isNotEmpty()) Text(marks.joinToString(" · "), style = MaterialTheme.typography.labelSmall)
                hit.hits.forEach { h -> Text(snippet(h.parts), maxLines = 2, overflow = TextOverflow.Ellipsis) }
            }
        },
        colors = ListItemDefaults.colors(
            containerColor = if (hit.threadId == selected) MaterialTheme.colorScheme.primaryContainer else MaterialTheme.colorScheme.surface,
        ),
        modifier = Modifier.clickable { actions.open(hit.threadId, hit.kai, hit.hits.firstNotNullOfOrNull { it.messageId }) },
    )
}

/** A snippet's parts with the matches in bold: plain text, never markup. */
private fun snippet(parts: List<SnippetPart>): AnnotatedString = buildAnnotatedString {
    parts.forEach { p ->
        if (p.hit) withStyle(SpanStyle(fontWeight = FontWeight.Bold)) { append(p.text) } else append(p.text)
    }
}

/** The steps for a folder without a model (§6.1's `folder_no_model`), in the dashboard's words. */
@Composable
fun NoModelDialog(steps: String, onDismiss: () -> Unit) {
    AlertDialog(
        onDismissRequest = onDismiss,
        title = { Text(stringResource(R.string.threads_no_model_title)) },
        text = { Text(steps) },
        confirmButton = { TextButton(onClick = onDismiss) { Text(stringResource(R.string.ok)) } },
    )
}
