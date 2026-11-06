package dev.rootshell.kai.ui.threads

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.imePadding
import androidx.compose.foundation.layout.padding
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
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
import dev.rootshell.kai.app.ThreadVoice
import dev.rootshell.kai.core.ffi.CallStart
import dev.rootshell.kai.core.ffi.CoreException
import dev.rootshell.kai.core.ffi.OpenedStatus
import dev.rootshell.kai.core.ffi.OpenedView
import dev.rootshell.kai.ui.conversation.FollowEnd
import dev.rootshell.kai.ui.conversation.MessageInput
import dev.rootshell.kai.ui.conversation.MessagesColumn
import dev.rootshell.kai.ui.voice.rememberTalk
import dev.rootshell.kai.ui.voice.stay
import kotlinx.coroutines.launch

/** The opened thread's facts as its menu acts on them. */
fun OpenedView.menu() = MenuThread(threadId, title, folderId, pinned, archived, temporary)

/**
 * The opened thread (design §6.4): its messages as the Kai conversation renders them, with the
 * approval cards at their messages, banners (why it is gone or was not read, who else answers, who
 * holds its voice, the hold) and the text field. Voice (§7.12): Talk beside the field binds a call
 * to this thread; while the phone's call runs, a line says where voice is ([VoiceLine]). [header]
 * draws the title and the thread's menu (a wide window's detail pane; on a compact one the top
 * bar has them).
 */
@Composable
fun ThreadPane(
    app: KaiApplication,
    header: Boolean,
    onBackToKai: () -> Unit,
    onShowCall: () -> Unit,
    onError: (String) -> Unit,
    modifier: Modifier = Modifier,
) {
    val hub = app.hub
    val opened by hub.opened.collectAsStateWithLifecycle()
    val messages by hub.openedMessages.collectAsStateWithLifecycle()
    val approvals by hub.approvals.collectAsStateWithLifecycle()
    val connection by hub.connection.collectAsStateWithLifecycle()
    val voice by hub.voice.collectAsStateWithLifecycle()
    val selected by hub.selected.collectAsStateWithLifecycle()
    val browser by hub.browser.collectAsStateWithLifecycle()
    val callError by hub.callError.collectAsStateWithLifecycle()
    val view = opened ?: return
    val scope = rememberCoroutineScope()
    val talk = rememberTalk(app)
    // The last call's error is shown here only after a Talk from this thread.
    var talked by remember(view.threadId) { mutableStateOf(false) }
    val voiceHere = ThreadRules.voice(voice, view)
    val talkHere = {
        talked = true
        talk(CallStart(new = false, takeOver = false, thread = view.threadId))
    }
    // The draft is the hub's, per thread: a fold moves this pane to another place in the
    // composition, and the compact window leaves it for the list and comes back.
    val ui = hub.threadsUi
    val draft = ui.draft(view.threadId)
    var error by remember(view.threadId) { mutableStateOf<String?>(null) }
    val follow = remember(view.threadId) { FollowEnd() }
    Column(modifier.imePadding()) {
        if (header) {
            Row(Modifier.fillMaxWidth().padding(start = 16.dp, end = 4.dp, top = 8.dp, bottom = 8.dp), verticalAlignment = Alignment.CenterVertically) {
                Text(
                    view.title.ifBlank { stringResource(R.string.threads_untitled) },
                    style = MaterialTheme.typography.titleLarge,
                    maxLines = 1,
                    overflow = TextOverflow.Ellipsis,
                    modifier = Modifier.weight(1f),
                )
                ThreadMenuButton(app, view.menu(), browser?.folders.orEmpty(), onError)
            }
        }
        val lines = buildList {
            view.note?.let { add(it) }
            if (view.status == OpenedStatus.LOADING && messages.isEmpty()) add(stringResource(R.string.thread_loading))
            view.answering.forEach { add(stringResource(R.string.answering, it)) }
            view.voiceBy?.let { add(stringResource(R.string.voice_on, it)) }
            if (connection?.held == true) add(stringResource(R.string.hold_badge))
        }
        lines.forEach { Banner(it) }
        VoiceLine(
            voiceHere,
            onBackToKai = onBackToKai,
            onBackTo = { hub.select(ThreadTarget(it)) },
            onTalkHere = talkHere,
            onShowCall = onShowCall,
            onStay = {
                talked = true
                voice?.call?.let { stay(app, it) }
            },
        )
        if (talked) callError?.let { Text(it, color = MaterialTheme.colorScheme.error, style = MaterialTheme.typography.bodySmall, modifier = Modifier.padding(horizontal = 16.dp)) }
        MessagesColumn(
            app = app,
            messages = messages,
            approvals = approvals.filter { it.threadId == view.threadId },
            follow = follow,
            restartKey = view.threadId,
            empty = if (view.status == OpenedStatus.READY) stringResource(R.string.thread_no_messages) else null,
            watch = "thread",
            modifier = Modifier.weight(1f).fillMaxWidth(),
            scrollTo = selected?.takeIf { it.id == view.threadId }?.messageId,
        )
        error?.let {
            Text(it, color = MaterialTheme.colorScheme.error, style = MaterialTheme.typography.bodySmall, modifier = Modifier.padding(horizontal = 16.dp))
        }
        if (view.status != OpenedStatus.GONE) {
            val note = if (ThreadRules.sendStopsVoice(voice, view.threadId, view.kai)) stringResource(R.string.send_stops_voice) else null
            val leading: (@Composable () -> Unit)? = if (voiceHere == ThreadVoice.Idle(talk = true)) {
                { TextButton(onClick = talkHere) { Text(stringResource(R.string.talk)) } }
            } else {
                null
            }
            MessageInput(draft, { ui.setDraft(view.threadId, it) }, sending = view.sending, note = note, leading = leading) {
                val text = draft
                val thread = view.threadId
                ui.setDraft(thread, "")
                error = null
                follow.on = true
                scope.launch {
                    try {
                        app.core.sendInThread(thread, text)
                    } catch (e: CoreException) {
                        error = ThreadRules.words(e)
                        // A send that never started keeps its text.
                        if (ui.draft(thread).isEmpty() && (e is CoreException.NotConnected || e is CoreException.NotReady)) ui.setDraft(thread, text)
                    }
                }
            }
        }
    }
}

/**
 * Where the phone's call is, seen from this thread (design §7.12): bound elsewhere, with the way
 * back to it and Talk here (or that the call moves here); bound here, with Show call and, while a
 * Talk elsewhere waits, Stay here. Nothing without a call.
 */
@Composable
private fun VoiceLine(
    voice: ThreadVoice,
    onBackToKai: () -> Unit,
    onBackTo: (Long) -> Unit,
    onTalkHere: () -> Unit,
    onShowCall: () -> Unit,
    onStay: () -> Unit,
) {
    val (text, buttons) = when (voice) {
        is ThreadVoice.Idle -> return
        is ThreadVoice.Elsewhere -> {
            val words = when {
                voice.coming -> stringResource(R.string.thread_voice_coming)
                voice.threadId == null -> stringResource(R.string.thread_voice_elsewhere)
                voice.title != null -> stringResource(R.string.thread_voice_with, voice.title)
                else -> stringResource(R.string.thread_voice_with_other)
            }
            words to @Composable {
                val back = voice.threadId
                if (back == null) {
                    TextButton(onClick = onBackToKai) { Text(stringResource(R.string.thread_back_to_kai)) }
                } else {
                    TextButton(onClick = { onBackTo(back) }) { Text(stringResource(R.string.thread_back_to_it)) }
                }
                if (voice.talk && !voice.coming) TextButton(onClick = onTalkHere) { Text(stringResource(R.string.thread_talk_here)) }
            }
        }
        is ThreadVoice.Here -> {
            val words = stringResource(if (voice.leaving) R.string.thread_voice_leaving else R.string.thread_voice_here)
            words to @Composable {
                if (voice.leaving) TextButton(onClick = onStay) { Text(stringResource(R.string.thread_voice_stay)) }
                TextButton(onClick = onShowCall) { Text(stringResource(R.string.thread_show_call)) }
            }
        }
    }
    Surface(color = MaterialTheme.colorScheme.secondaryContainer, modifier = Modifier.fillMaxWidth().padding(horizontal = 16.dp, vertical = 2.dp), shape = MaterialTheme.shapes.small) {
        // The buttons under the words, so they fit the cover screen at a large font.
        Column(Modifier.padding(start = 12.dp, top = 8.dp)) {
            Text(text, style = MaterialTheme.typography.bodyMedium, modifier = Modifier.padding(end = 12.dp))
            Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.End) { buttons() }
        }
    }
}

@Composable
fun Banner(text: String) {
    Surface(color = MaterialTheme.colorScheme.surfaceContainer, modifier = Modifier.fillMaxWidth().padding(horizontal = 16.dp, vertical = 2.dp), shape = MaterialTheme.shapes.small) {
        Text(text, style = MaterialTheme.typography.bodyMedium, modifier = Modifier.padding(12.dp))
    }
}
