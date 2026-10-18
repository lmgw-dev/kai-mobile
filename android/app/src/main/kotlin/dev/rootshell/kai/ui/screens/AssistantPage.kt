package dev.rootshell.kai.ui.screens

import android.Manifest
import android.app.role.RoleManager
import android.content.Context
import android.content.Intent
import android.provider.Settings
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.padding
import androidx.compose.material3.Checkbox
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.LifecycleResumeEffect
import dev.rootshell.kai.R
import dev.rootshell.kai.ui.LabelledCheckbox
import dev.rootshell.kai.ui.SetupStep
import dev.rootshell.kai.assist.AssistantSetup
import dev.rootshell.kai.assist.AssistantStep
import dev.rootshell.kai.assist.Maker
import dev.rootshell.kai.assist.SetKai

/**
 * Setup's "Kai as your digital assistant" card (design §9.5), a settings page as WP2's background
 * card is: what changes, Set Kai (the role, re-checked on resume), the button's steps for this
 * phone's maker (ticked by the owner; LineageOS's power button setting is read), and the microphone,
 * asked here because an invocation should listen, not show a permission dialog. Assistant mode is
 * an option: the tile and the shortcut start a call without it.
 */
@Composable
fun AssistantPage(modifier: Modifier = Modifier) {
    val context = LocalContext.current
    var facts by remember { mutableStateOf(AssistantSetup.facts(context)) }
    LifecycleResumeEffect(Unit) {
        facts = AssistantSetup.facts(context)
        onPauseOrDispose {}
    }
    val refresh = { facts = AssistantSetup.facts(context) }
    val askMicrophone = rememberLauncherForActivityResult(ActivityResultContracts.RequestPermission()) { refresh() }
    val askRole = rememberLauncherForActivityResult(ActivityResultContracts.StartActivityForResult()) { refresh() }
    val open = AssistantSetup.open(facts)
    Column(modifier.padding(horizontal = 16.dp), verticalArrangement = Arrangement.spacedBy(12.dp)) {
        Text(stringResource(R.string.assistant_intro), style = MaterialTheme.typography.bodyMedium)
        SetupStep(stringResource(R.string.assistant_role), done = facts.roleHeld) {
            when (AssistantSetup.SET_KAI) {
                SetKai.ROLE_REQUEST -> context.getSystemService(RoleManager::class.java)
                    ?.createRequestRoleIntent(RoleManager.ROLE_ASSISTANT)
                    ?.let { askRole.launch(it) }
                SetKai.VOICE_INPUT_SETTINGS -> openSettings(context, Settings.ACTION_VOICE_INPUT_SETTINGS)
            }
        }
        if (!facts.roleHeld) {
            OutlinedButton(onClick = { openSettings(context, Settings.ACTION_MANAGE_DEFAULT_APPS_SETTINGS) }) {
                Text(stringResource(R.string.assistant_default_apps))
            }
            if (!facts.roleAvailable) Note(stringResource(R.string.assistant_role_unavailable))
        }
        Text(
            stringResource(
                when (facts.maker) {
                    Maker.SAMSUNG -> R.string.assistant_button_samsung
                    Maker.LINEAGE -> R.string.assistant_button_lineage
                    Maker.PIXEL -> R.string.assistant_button_pixel
                    Maker.OTHER -> R.string.assistant_button_other
                },
            ),
            style = MaterialTheme.typography.bodyMedium,
        )
        if (facts.maker == Maker.LINEAGE) {
            Note(
                facts.powerLongPress?.let { stringResource(R.string.assistant_power_value, it, AssistantSetup.POWER_ASSISTANT) }
                    ?: stringResource(if (facts.powerUnreadable) R.string.assistant_power_unreadable else R.string.assistant_power_unset),
            )
        }
        LabelledCheckbox(stringResource(R.string.assistant_button_done), AssistantSetup.buttonDone(facts)) {
            AssistantSetup.tickButton(context, it)
            refresh()
        }
        SetupStep(stringResource(R.string.assistant_microphone), done = facts.microphone) {
            askMicrophone.launch(Manifest.permission.RECORD_AUDIO)
        }
        Text(
            stringResource(if (open.isEmpty()) R.string.assistant_done else R.string.assistant_open),
            style = MaterialTheme.typography.labelLarge,
        )
        if (AssistantStep.Role in open) Note(stringResource(R.string.assistant_without_role))
    }
}

@Composable
private fun Note(text: String) {
    Text(text, style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
}

/** A system settings page; a phone without it opens the default apps instead. */
private fun openSettings(context: Context, action: String) {
    runCatching { context.startActivity(Intent(action)) }
        .onFailure { runCatching { context.startActivity(Intent(Settings.ACTION_MANAGE_DEFAULT_APPS_SETTINGS)) } }
}
