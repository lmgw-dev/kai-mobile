package dev.rootshell.kai.ui.conversation

import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.imePadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.gestures.scrollBy
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.LazyListState
import androidx.compose.foundation.lazy.rememberLazyListState
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.Send
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.runtime.snapshotFlow
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.semantics.text
import androidx.compose.ui.text.AnnotatedString
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import dev.rootshell.kai.KaiApplication
import dev.rootshell.kai.R
import dev.rootshell.kai.core.ffi.ConversationStatus
import dev.rootshell.kai.core.ffi.CoreException
import dev.rootshell.kai.core.ffi.MessageView
import dev.rootshell.kai.core.ffi.Role
import dev.rootshell.kai.ui.ScrollFrameWatch
import dev.rootshell.kai.ui.approvals.ApprovalCard
import dev.rootshell.kai.ui.approvals.ApprovalRules
import dev.rootshell.kai.ui.approvals.rememberApprovalDecider
import kotlinx.coroutines.flow.distinctUntilChanged
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.launch

/**
 * The Kai conversation (design §6, §10.1): the desktop's thread, its messages with Markdown and the
 * fallback's mark, the reply streaming to the phone's own send, banners (the folder's steps, who
 * else answers, who holds the voice, the hold), the calls waiting for a decision as cards at their
 * messages (§11), the text field following the IME, and New conversation. Everything comes from
 * the core's state.
 *
 * The list follows its end (a new message, the streaming reply's growth) only while the owner is
 * at the end: scrolled up to read, it stays where the owner put it, until another thread or call
 * shows or the owner sends a message. Each scroll's frames are measured against the display's
 * budget and logged (§10.4).
 */
@Composable
fun ConversationPane(app: KaiApplication, modifier: Modifier = Modifier) {
    val hub = app.hub
    val core = app.core
    val conversation by hub.conversation.collectAsStateWithLifecycle()
    val messages by hub.messages.collectAsStateWithLifecycle()
    val approvals by hub.approvals.collectAsStateWithLifecycle()
    val connection by hub.connection.collectAsStateWithLifecycle()
    val decider = rememberApprovalDecider(app)
    val scope = rememberCoroutineScope()
    var draft by rememberSaveable { mutableStateOf("") }
    var error by remember { mutableStateOf<String?>(null) }
    val list = rememberLazyListState()
    val placement = ApprovalRules.place(approvals, messages.map { it.id })
    // Following the end: set by where each scroll left the list (the follow's own scroll ends at
    // the end, so it stays set), and again when another thread or call shows and at the owner's
    // own send.
    var follow by remember { mutableStateOf(true) }
    val call by remember(hub) { hub.voice.map { it?.call }.distinctUntilChanged() }.collectAsStateWithLifecycle(null)
    LaunchedEffect(conversation?.threadId, call) { follow = true }
    LaunchedEffect(list) {
        snapshotFlow { list.isScrollInProgress }.collect { scrolling -> if (!scrolling) follow = !list.canScrollForward }
    }
    LaunchedEffect(messages.size, messages.lastOrNull()?.text?.length, approvals.size, follow) {
        if (follow) followEnd(list)
    }
    ScrollFrameWatch(list, "conversation")
    Column(modifier.imePadding()) {
        Row(Modifier.fillMaxWidth().padding(horizontal = 16.dp, vertical = 8.dp), verticalAlignment = Alignment.CenterVertically) {
            Text(
                conversation?.threadTitle ?: stringResource(R.string.conversation_title),
                style = MaterialTheme.typography.titleLarge,
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
                modifier = Modifier.weight(1f),
            )
            if (conversation?.status == ConversationStatus.READY) {
                TextButton(onClick = {
                    scope.launch {
                        error = try {
                            core.newConversation()
                            null
                        } catch (e: CoreException) {
                            e.message
                        }
                    }
                }) { Text(stringResource(R.string.new_conversation)) }
            }
        }
        Banners(conversation, connection?.held == true, connection?.paired == true)
        LazyColumn(state = list, modifier = Modifier.weight(1f).fillMaxWidth(), contentPadding = PaddingValues(16.dp)) {
            if (messages.isEmpty() && conversation?.status == ConversationStatus.READY) {
                item { Text(stringResource(R.string.conversation_no_messages), color = MaterialTheme.colorScheme.onSurfaceVariant) }
            }
            messages.forEach { m ->
                item(key = m.id, contentType = "message") { MessageRow(m) }
                placement.after[m.id]?.forEach { a ->
                    item(key = "approval:${a.approvalRequestId}", contentType = "approval") {
                        ApprovalCard(a, decider, Modifier.padding(vertical = 4.dp))
                    }
                }
            }
            placement.end.forEach { a ->
                item(key = "approval:${a.approvalRequestId}", contentType = "approval") {
                    ApprovalCard(a, decider, Modifier.padding(vertical = 4.dp))
                }
            }
        }
        error?.let {
            Text(it, color = MaterialTheme.colorScheme.error, style = MaterialTheme.typography.bodySmall, modifier = Modifier.padding(horizontal = 16.dp))
        }
        Row(Modifier.fillMaxWidth().padding(8.dp), verticalAlignment = Alignment.CenterVertically) {
            OutlinedTextField(
                value = draft,
                onValueChange = { draft = it },
                placeholder = { Text(stringResource(R.string.message_hint)) },
                modifier = Modifier.weight(1f),
                maxLines = 6,
            )
            val sending = conversation?.sending == true
            IconButton(
                enabled = draft.isNotBlank() && !sending,
                onClick = {
                    val text = draft
                    draft = ""
                    error = null
                    follow = true
                    scope.launch {
                        try {
                            core.sendText(text)
                        } catch (e: CoreException) {
                            error = e.message
                            // A send that never started keeps its text.
                            if (draft.isEmpty() && (e is CoreException.NotConnected || e is CoreException.NotReady)) draft = text
                        }
                    }
                },
            ) { Icon(Icons.AutoMirrored.Filled.Send, contentDescription = stringResource(R.string.send)) }
        }
    }
}

@Composable
private fun Banners(conversation: dev.rootshell.kai.core.ffi.ConversationView?, held: Boolean, paired: Boolean) {
    val lines = buildList {
        if (!paired) add(stringResource(R.string.conversation_empty))
        when (conversation?.status) {
            ConversationStatus.LOADING -> if (paired) add(stringResource(R.string.conversation_loading, conversation.folderName))
            ConversationStatus.NOT_READY -> conversation.steps?.let { add(it) }
            ConversationStatus.FAILED -> conversation.note?.let { add(it) }
            else -> {}
        }
        conversation?.answering?.forEach { add(stringResource(R.string.answering, it)) }
        conversation?.voiceBy?.let { add(stringResource(R.string.voice_on, it)) }
        if (held) add(stringResource(R.string.hold_badge))
    }
    lines.forEach {
        Surface(color = MaterialTheme.colorScheme.surfaceContainer, modifier = Modifier.fillMaxWidth().padding(horizontal = 16.dp, vertical = 2.dp), shape = MaterialTheme.shapes.small) {
            Text(it, style = MaterialTheme.typography.bodyMedium, modifier = Modifier.padding(12.dp))
        }
    }
}

@Composable
private fun MessageRow(m: MessageView) {
    val mine = m.role == Role.USER
    // TalkBack reads who said it first, then the message as one item (its links stay their own).
    val who = stringResource(if (mine) R.string.caption_owner else R.string.caption_kai)
    Column(
        Modifier
            .fillMaxWidth()
            .padding(vertical = 6.dp)
            .semantics(mergeDescendants = true) { text = AnnotatedString(who) },
        horizontalAlignment = if (mine) Alignment.End else Alignment.Start,
    ) {
        Surface(
            color = if (mine) MaterialTheme.colorScheme.primaryContainer else MaterialTheme.colorScheme.surfaceContainer,
            shape = MaterialTheme.shapes.medium,
            modifier = Modifier.widthIn(max = 560.dp),
        ) {
            Column(Modifier.padding(12.dp)) {
                if (m.tools.isNotEmpty()) {
                    Text(stringResource(R.string.tools_called, m.tools.joinToString(", ")), style = MaterialTheme.typography.labelSmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
                }
                if (m.blocks.isEmpty() && m.streaming) {
                    Text("…", style = MaterialTheme.typography.bodyLarge)
                } else {
                    Markdown(m.blocks)
                }
                m.error?.let { Text(it, color = MaterialTheme.colorScheme.error, style = MaterialTheme.typography.bodySmall) }
            }
        }
        // A tool label the answer went without: a quiet line, in the conversation only (never in a
        // notification).
        m.notices.forEach {
            Text(it, style = MaterialTheme.typography.labelSmall, color = MaterialTheme.colorScheme.onSurfaceVariant, modifier = Modifier.padding(top = 2.dp))
        }
        val marks = buildList {
            if (m.voice) add(stringResource(R.string.mark_voice))
            m.answeredBy?.let { add(stringResource(R.string.mark_fallback, it)) }
        }
        if (marks.isNotEmpty()) {
            Text(marks.joinToString(" · "), style = MaterialTheme.typography.labelSmall, color = MaterialTheme.colorScheme.onSurfaceVariant, modifier = Modifier.padding(top = 2.dp))
        }
    }
}

/**
 * Scrolls to the end of the last item (a long streaming reply's newest words), without an animation
 * per streamed delta: to the last item, then a viewport at a time until the list can go no further.
 */
suspend fun followEnd(list: LazyListState) {
    val last = list.layoutInfo.totalItemsCount - 1
    if (last < 0) return
    list.scrollToItem(last)
    while (list.canScrollForward) {
        val step = list.layoutInfo.viewportSize.height.toFloat().coerceAtLeast(1f)
        if (list.scrollBy(step) == 0f) break
    }
}
