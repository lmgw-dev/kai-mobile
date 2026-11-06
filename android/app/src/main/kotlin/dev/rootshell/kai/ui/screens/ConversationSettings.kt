package dev.rootshell.kai.ui.screens

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.material3.Button
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.unit.dp
import android.os.Build
import android.provider.Settings
import dev.rootshell.kai.KaiApplication
import dev.rootshell.kai.R
import dev.rootshell.kai.core.ffi.CoreException
import dev.rootshell.kai.ui.ChoiceRow
import dev.rootshell.kai.ui.SectionHeader
import dev.rootshell.kai.ui.SupportingText
import kotlinx.coroutines.launch

/**
 * Where the Kai conversation lives (design §3.4, §6.1; 2026-10-19). By default every Kai app
 * shares the Chat folder "Kai"; a phone may keep a conversation of its own in another folder. Saving
 * hands the core the new name: it looks the folder up again and clears the old conversation from
 * the screen at once (`Core::set_settings` → `settings_changed`), so nothing is restarted by hand.
 */
@Composable
fun ConversationPage(app: KaiApplication, modifier: Modifier) {
    val scope = rememberCoroutineScope()
    val stored = remember { app.core.settings().conversation.folderName }
    var choice by remember { mutableStateOf(ConversationName.choiceOf(stored)) }
    var typed by remember { mutableStateOf(ConversationName.fieldStart(stored, deviceName(app))) }
    var said by remember { mutableStateOf<String?>(null) }
    val refusal = ConversationName.refusal(choice, typed)
    Column(modifier.padding(horizontal = 16.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
        SupportingText(stringResource(R.string.set_conversation_intro))
        SectionHeader(stringResource(R.string.set_conversation_where))
        ChoiceRow(
            stringResource(R.string.set_conversation_shared),
            choice == ConversationChoice.SHARED,
            stringResource(R.string.set_conversation_shared_help),
        ) { choice = ConversationChoice.SHARED }
        ChoiceRow(
            stringResource(R.string.set_conversation_own),
            choice == ConversationChoice.THIS_PHONE,
            stringResource(R.string.set_conversation_own_help),
        ) { choice = ConversationChoice.THIS_PHONE }
        if (choice == ConversationChoice.THIS_PHONE) {
            OutlinedTextField(
                value = typed,
                onValueChange = { typed = it },
                label = { Text(stringResource(R.string.set_conversation_name)) },
                supportingText = {
                    Text(
                        stringResource(
                            when (refusal) {
                                NameRefusal.EMPTY -> R.string.set_conversation_name_empty
                                NameRefusal.IS_SHARED -> R.string.set_conversation_name_shared
                                null -> R.string.set_conversation_name_help
                            },
                        ),
                    )
                },
                isError = refusal != null,
                singleLine = true,
                modifier = Modifier.fillMaxWidth(),
            )
            SupportingText(stringResource(R.string.set_conversation_model_note))
        }
        Button(
            enabled = refusal == null,
            onClick = {
                val name = ConversationName.folderFor(choice, typed) ?: return@Button
                scope.launch {
                    said = try {
                        val s = app.core.settings()
                        s.conversation.folderName = name
                        app.core.setSettings(s)
                        app.getString(R.string.set_conversation_saved)
                    } catch (e: CoreException) {
                        refusalWords(app, e)
                    }
                }
            },
        ) { Text(stringResource(R.string.save)) }
        said?.let { Text(it, style = MaterialTheme.typography.bodyMedium) }
    }
}

/** The name the owner gave this phone (Settings → About phone), else the model name. */
private fun deviceName(app: KaiApplication): String =
    Settings.Global.getString(app.contentResolver, Settings.Global.DEVICE_NAME)
        ?.takeIf { it.isNotBlank() } ?: Build.MODEL
