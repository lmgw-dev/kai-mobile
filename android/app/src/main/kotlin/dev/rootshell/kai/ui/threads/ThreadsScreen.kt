package dev.rootshell.kai.ui.threads

import androidx.activity.compose.BackHandler
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.consumeWindowInsets
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.widthIn
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.ArrowBack
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Scaffold
import androidx.compose.material3.SnackbarHost
import androidx.compose.material3.SnackbarHostState
import androidx.compose.material3.Text
import androidx.compose.material3.TopAppBar
import androidx.compose.material3.VerticalDivider
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import dev.rootshell.kai.KaiApplication
import dev.rootshell.kai.R
import dev.rootshell.kai.app.ThreadRules
import dev.rootshell.kai.app.ThreadTarget
import dev.rootshell.kai.core.ffi.CoreException
import dev.rootshell.kai.ui.Layout
import dev.rootshell.kai.ui.ScreenInsets
import dev.rootshell.kai.ui.currentLayout
import kotlinx.coroutines.launch

/**
 * The threads browser (design §6.4, §10.6), reached from the conversation's top bar. Compact width
 * (a foldable's cover screen): the list, then the opened thread as its second page, as settings'
 * list and page are (back returns to the list, then leaves). Wide: list and detail side by side,
 * the list at a readable measure and the opened thread at the conversation's 640 dp. The opened
 * thread is the hub's ([dev.rootshell.kai.app.CoreHub.selected]) and the core's, so a fold or an
 * unfold keeps it. The Kai thread opens as the Kai conversation ([onOpenKai]).
 */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun ThreadsScreen(app: KaiApplication, onBack: () -> Unit, onOpenKai: () -> Unit, onShowCall: () -> Unit) {
    val hub = app.hub
    val layout = currentLayout()
    val selected by hub.selected.collectAsStateWithLifecycle()
    val opened by hub.opened.collectAsStateWithLifecycle()
    val browser by hub.browser.collectAsStateWithLifecycle()
    val snackbar = remember { SnackbarHostState() }
    val scope = rememberCoroutineScope()
    var steps by remember { mutableStateOf<String?>(null) }
    val onError: (String) -> Unit = { scope.launch { snackbar.showSnackbar(it) } }
    val page = layout == Layout.Compact && selected != null

    // The Kai thread is the Kai conversation, never a second copy: an opening the core marked so
    // (the conversation was still loading when it was picked) leaves for the conversation.
    val opensKai = opened?.kai == true
    LaunchedEffect(opensKai) {
        if (opensKai) {
            hub.select(null)
            onOpenKai()
        }
    }

    // On a compact screen, back from a thread returns to the list; on the list it leaves.
    BackHandler(enabled = page) { hub.select(null) }

    val actions = BrowserActions(
        open = { id, kai, message -> if (kai) onOpenKai() else hub.select(ThreadTarget(id, message)) },
        newIn = { section ->
            when {
                section.kai -> scope.launch {
                    try {
                        app.core.newConversation()
                        onOpenKai()
                    } catch (e: CoreException) {
                        onError(ThreadRules.words(e))
                    }
                }
                !section.canCreate -> steps = section.steps
                else -> section.folderId?.let { folder ->
                    scope.launch {
                        try {
                            val id = app.core.newThread(folder)
                            hub.select(ThreadTarget(id))
                        } catch (e: CoreException) {
                            onError(ThreadRules.words(e))
                        }
                    }
                }
            }
        },
        onError = onError,
    )
    Scaffold(
        contentWindowInsets = ScreenInsets.content(),
        topBar = {
            TopAppBar(
                windowInsets = ScreenInsets.bar(),
                title = {
                    val title = opened?.takeIf { page }?.title?.ifBlank { stringResource(R.string.threads_untitled) }
                    Text(title ?: stringResource(R.string.threads_title), maxLines = 1, overflow = TextOverflow.Ellipsis)
                },
                navigationIcon = {
                    IconButton(onClick = { if (page) hub.select(null) else onBack() }) {
                        Icon(Icons.AutoMirrored.Filled.ArrowBack, contentDescription = stringResource(R.string.back))
                    }
                },
                actions = {
                    val o = opened
                    if (page && o != null) ThreadMenuButton(app, o.menu(), browser?.folders.orEmpty(), onError)
                },
            )
        },
        snackbarHost = { SnackbarHost(snackbar) },
    ) { padding ->
        // Consumed, so the thread's own IME padding counts only what the Scaffold left.
        val body = Modifier.fillMaxSize().padding(padding).consumeWindowInsets(padding)
        when {
            layout == Layout.Wide -> Row(body) {
                BrowserList(app, selected?.id, actions, Modifier.weight(0.42f).fillMaxHeight())
                VerticalDivider()
                Box(Modifier.weight(0.58f).fillMaxHeight(), contentAlignment = Alignment.TopCenter) {
                    if (selected == null || opened == null) {
                        Text(
                            stringResource(R.string.threads_pick),
                            color = MaterialTheme.colorScheme.onSurfaceVariant,
                            modifier = Modifier.padding(24.dp),
                        )
                    } else {
                        ThreadPane(app, header = true, onBackToKai = onOpenKai, onShowCall = onShowCall, onError = onError, modifier = Modifier.widthIn(max = 640.dp).fillMaxSize())
                    }
                }
            }
            page -> Box(body) { ThreadPane(app, header = false, onBackToKai = onOpenKai, onShowCall = onShowCall, onError = onError, modifier = Modifier.fillMaxSize()) }
            else -> BrowserList(app, null, actions, body)
        }
    }
    steps?.let { NoModelDialog(it) { steps = null } }
}
