package dev.rootshell.kai.ui.screens

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.material3.Button
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.RadioButton
import androidx.compose.material3.Switch
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.unit.dp
import dev.rootshell.kai.KaiApplication
import dev.rootshell.kai.R
import dev.rootshell.kai.ui.LabelledRadio
import dev.rootshell.kai.ui.LabelledSwitch
import dev.rootshell.kai.core.ffi.CoreException
import dev.rootshell.kai.core.ffi.EchoMode
import dev.rootshell.kai.core.ffi.Route
import dev.rootshell.kai.core.ffi.VoiceMode
import kotlinx.coroutines.launch

/**
 * The voice settings (design §3.4, §10.1): whose turn-taking the call follows, the waits, the
 * playout reports, push-to-talk's tail with the rule beside it (it must fit inside lmgw's
 * `webrtc_commit_window_ms`, §7.5), the default output, the echo canceller or half duplex with its
 * tail (§8.2), the cue. Each value is the core's; a value it refuses is said with its key.
 */
@Composable
fun VoicePage(app: KaiApplication, modifier: Modifier) {
    val scope = rememberCoroutineScope()
    val start = remember { app.core.settings() }
    var mode by remember { mutableStateOf(start.voice.mode) }
    var connect by remember { mutableStateOf(start.voice.connectTimeoutS.toString()) }
    var idle by remember { mutableStateOf(start.voice.idleEndS.toString()) }
    var report by remember { mutableStateOf(start.voice.playoutReportMs.toString()) }
    var tail by remember { mutableStateOf(start.voice.pttTailMs?.toString().orEmpty()) }
    var route by remember { mutableStateOf(start.voice.route) }
    var echo by remember { mutableStateOf(start.voice.echo) }
    var echoTail by remember { mutableStateOf(start.voice.echoTailMs?.toString().orEmpty()) }
    var earcon by remember { mutableStateOf(start.voice.earcon) }
    var haptics by remember { mutableStateOf(start.voice.haptics) }
    var said by remember { mutableStateOf<String?>(null) }
    Column(modifier.padding(horizontal = 16.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
        Text("voice.mode", style = MaterialTheme.typography.labelLarge)
        Choice(stringResource(R.string.voice_mode_thread), mode == VoiceMode.THREAD) { mode = VoiceMode.THREAD }
        Choice(stringResource(R.string.voice_mode_automatic), mode == VoiceMode.AUTOMATIC) { mode = VoiceMode.AUTOMATIC }
        Choice(stringResource(R.string.voice_mode_ptt), mode == VoiceMode.PUSH_TO_TALK) { mode = VoiceMode.PUSH_TO_TALK }
        Field("voice.connect_timeout_s", R.string.setting_connect_timeout, connect) { connect = it }
        Field("voice.idle_end_s", R.string.setting_idle_end, idle) { idle = it }
        Field("voice.playout_report_ms", R.string.setting_playout_report, report) { report = it }
        Field("voice.ptt_tail_ms", R.string.setting_ptt_tail, tail) { tail = it }
        Text("voice.route", style = MaterialTheme.typography.labelLarge)
        Choice(stringResource(R.string.voice_route_speaker), route == Route.SPEAKER) { route = Route.SPEAKER }
        Choice(stringResource(R.string.voice_route_earpiece), route == Route.EARPIECE) { route = Route.EARPIECE }
        Text("voice.echo", style = MaterialTheme.typography.labelLarge)
        Choice(stringResource(R.string.voice_echo_auto), echo == EchoMode.AUTO) { echo = EchoMode.AUTO }
        Choice(stringResource(R.string.voice_echo_hardware), echo == EchoMode.HARDWARE) { echo = EchoMode.HARDWARE }
        Choice(stringResource(R.string.voice_echo_software), echo == EchoMode.SOFTWARE) { echo = EchoMode.SOFTWARE }
        Choice(stringResource(R.string.voice_echo_half_duplex), echo == EchoMode.HALF_DUPLEX) { echo = EchoMode.HALF_DUPLEX }
        Text(stringResource(R.string.voice_echo_note), style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
        Field("voice.echo_tail_ms", R.string.setting_echo_tail, echoTail) { echoTail = it }
        Toggle("voice.earcon", earcon) { earcon = it }
        Toggle("voice.haptics", haptics) { haptics = it }
        Button(onClick = {
            scope.launch {
                said = try {
                    val s = app.core.settings()
                    s.voice.mode = mode
                    s.voice.connectTimeoutS = connect.trim().toUInt()
                    s.voice.idleEndS = idle.trim().toUInt()
                    s.voice.playoutReportMs = report.trim().toUInt()
                    s.voice.pttTailMs = tail.trim().takeIf { it.isNotEmpty() }?.toUInt()
                    s.voice.route = route
                    s.voice.echo = echo
                    s.voice.echoTailMs = echoTail.trim().takeIf { it.isNotEmpty() }?.toUInt()
                    s.voice.earcon = earcon
                    s.voice.haptics = haptics
                    app.core.setSettings(s)
                    // A changed echo setting rebuilds the factory now, not at the next call's start.
                    app.calls.refreshFactory()
                    app.getString(R.string.voice_saved)
                } catch (e: NumberFormatException) {
                    app.getString(R.string.voice_not_a_number)
                } catch (e: CoreException) {
                    e.message
                }
            }
        }) { Text(stringResource(R.string.save)) }
        said?.let { Text(it, style = MaterialTheme.typography.bodyMedium) }
    }
}

@Composable
private fun Field(key: String, help: Int, value: String, onChange: (String) -> Unit) {
    OutlinedTextField(
        value = value,
        onValueChange = onChange,
        label = { Text(key) },
        supportingText = { Text(stringResource(help)) },
        keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Number),
        singleLine = true,
        modifier = Modifier.fillMaxWidth(),
    )
}

@Composable
private fun Choice(label: String, selected: Boolean, onSelect: () -> Unit) = LabelledRadio(label, selected, onSelect)

@Composable
private fun Toggle(key: String, on: Boolean, onChange: (Boolean) -> Unit) = LabelledSwitch(key, on, onChange)
