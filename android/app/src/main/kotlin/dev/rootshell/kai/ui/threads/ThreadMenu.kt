package dev.rootshell.kai.ui.threads

import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.MoreVert
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.DropdownMenu
import androidx.compose.material3.DropdownMenuItem
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.ListItem
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.stringResource
import dev.rootshell.kai.KaiApplication
import dev.rootshell.kai.R
import dev.rootshell.kai.app.MoveTarget
import dev.rootshell.kai.app.ThreadAction
import dev.rootshell.kai.app.ThreadRules
import dev.rootshell.kai.core.ffi.CoreException
import dev.rootshell.kai.core.ffi.FolderChoice
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext

/** What a thread's menu acts on: a list row's or the opened thread's facts. */
data class MenuThread(
    val id: Long,
    val title: String,
    val folderId: Long?,
    val pinned: Boolean,
    val archived: Boolean,
    val temporary: Boolean,
)

/** Which dialog of a thread's menu is open. */
private sealed interface Dialog {
    data object Move : Dialog
    data object Rename : Dialog
    data object Delete : Dialog
}

/**
 * A thread's actions (design §6.4) as a menu that [expanded] opens (its overflow button, a long
 * press): pin or unpin, archive or restore, move (a folder picker), rename (a text dialog), delete
 * (asked first). Each
 * answers once lmgw did; a refusal reaches [onError] with its code. A temporary thread has none.
 */
@Composable
fun ThreadMenu(
    app: KaiApplication,
    thread: MenuThread,
    folders: List<FolderChoice>,
    expanded: Boolean,
    onDismiss: () -> Unit,
    onError: (String) -> Unit,
) {
    var dialog by remember { mutableStateOf<Dialog?>(null) }
    val run: (suspend () -> Unit) -> Unit = { action ->
        app.scope.launch {
            val why = try {
                action()
                null
            } catch (e: CoreException) {
                ThreadRules.words(e)
            }
            if (why != null) withContext(Dispatchers.Main) { onError(why) }
        }
    }
    val core = app.core
    DropdownMenu(expanded = expanded, onDismissRequest = onDismiss) {
        ThreadRules.actions(thread.pinned, thread.archived, thread.temporary).forEach { action ->
            DropdownMenuItem(
                text = { Text(stringResource(label(action))) },
                onClick = {
                    onDismiss()
                    when (action) {
                        ThreadAction.PIN -> run { core.pinThread(thread.id, true) }
                        ThreadAction.UNPIN -> run { core.pinThread(thread.id, false) }
                        ThreadAction.ARCHIVE -> run { core.archiveThread(thread.id, true) }
                        ThreadAction.RESTORE -> run { core.archiveThread(thread.id, false) }
                        ThreadAction.MOVE -> dialog = Dialog.Move
                        ThreadAction.RENAME -> dialog = Dialog.Rename
                        ThreadAction.DELETE -> dialog = Dialog.Delete
                    }
                },
            )
        }
    }
    val title = thread.title.ifBlank { stringResource(R.string.threads_untitled) }
    when (dialog) {
        Dialog.Move -> MoveDialog(title, ThreadRules.moveTargets(folders, thread.folderId), { dialog = null }) { target ->
            dialog = null
            run { core.moveThread(thread.id, target.folderId) }
        }
        Dialog.Rename -> RenameDialog(thread.title, { dialog = null }) { title ->
            dialog = null
            run { core.renameThread(thread.id, title) }
        }
        Dialog.Delete -> AlertDialog(
            onDismissRequest = { dialog = null },
            title = { Text(stringResource(R.string.thread_delete_title)) },
            text = { Text(stringResource(R.string.thread_delete_text, title)) },
            confirmButton = {
                TextButton(onClick = {
                    dialog = null
                    run { core.deleteThread(thread.id) }
                }) { Text(stringResource(R.string.thread_delete_confirm), color = MaterialTheme.colorScheme.error) }
            },
            dismissButton = { TextButton(onClick = { dialog = null }) { Text(stringResource(R.string.cancel)) } },
        )
        null -> Unit
    }
}

/** The overflow button that opens [ThreadMenu]; nothing for a thread without actions. */
@Composable
fun ThreadMenuButton(app: KaiApplication, thread: MenuThread, folders: List<FolderChoice>, onError: (String) -> Unit) {
    if (ThreadRules.actions(thread.pinned, thread.archived, thread.temporary).isEmpty()) return
    var open by remember { mutableStateOf(false) }
    Box {
        IconButton(onClick = { open = true }) {
            Icon(Icons.Default.MoreVert, contentDescription = stringResource(R.string.threads_menu))
        }
        ThreadMenu(app, thread, folders, open, { open = false }, onError)
    }
}

@Composable
private fun MoveDialog(title: String, targets: List<MoveTarget>, onDismiss: () -> Unit, onPick: (MoveTarget) -> Unit) {
    AlertDialog(
        onDismissRequest = onDismiss,
        title = { Text(stringResource(R.string.thread_move_title, title)) },
        text = {
            Column(Modifier.verticalScroll(rememberScrollState())) {
                targets.forEach { t ->
                    ListItem(
                        headlineContent = { Text(t.name ?: stringResource(R.string.threads_no_folder)) },
                        modifier = Modifier.clickable { onPick(t) },
                    )
                }
            }
        },
        confirmButton = {},
        dismissButton = { TextButton(onClick = onDismiss) { Text(stringResource(R.string.cancel)) } },
    )
}

/** The title in a text field; OK sends it as typed (lmgw trims it and refuses an empty one). */
@Composable
private fun RenameDialog(title: String, onDismiss: () -> Unit, onRename: (String) -> Unit) {
    var text by rememberSaveable { mutableStateOf(title) }
    AlertDialog(
        onDismissRequest = onDismiss,
        title = { Text(stringResource(R.string.thread_rename_title)) },
        text = { OutlinedTextField(value = text, onValueChange = { text = it }, singleLine = true, label = { Text(stringResource(R.string.thread_rename_label)) }) },
        confirmButton = { TextButton(enabled = text.isNotBlank() && text.trim() != title, onClick = { onRename(text) }) { Text(stringResource(R.string.ok)) } },
        dismissButton = { TextButton(onClick = onDismiss) { Text(stringResource(R.string.cancel)) } },
    )
}

private fun label(action: ThreadAction): Int = when (action) {
    ThreadAction.PIN -> R.string.thread_pin
    ThreadAction.UNPIN -> R.string.thread_unpin
    ThreadAction.ARCHIVE -> R.string.thread_archive
    ThreadAction.RESTORE -> R.string.thread_restore
    ThreadAction.MOVE -> R.string.thread_move
    ThreadAction.RENAME -> R.string.thread_rename
    ThreadAction.DELETE -> R.string.thread_delete
}
