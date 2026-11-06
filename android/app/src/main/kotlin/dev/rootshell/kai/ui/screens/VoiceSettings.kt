package dev.rootshell.kai.ui.screens

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.padding
import androidx.compose.material3.Button
import androidx.compose.material3.MaterialTheme
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
import dev.rootshell.kai.KaiApplication
import dev.rootshell.kai.R
import dev.rootshell.kai.ui.AdvancedGroup
import dev.rootshell.kai.ui.ChoiceRow
import dev.rootshell.kai.ui.SectionHeader
import dev.rootshell.kai.ui.SettingNumber
import dev.rootshell.kai.ui.SettingSwitch
import dev.rootshell.kai.ui.SupportingText
import dev.rootshell.kai.core.ffi.CoreException
import dev.rootshell.kai.core.ffi.EchoMode
import dev.rootshell.kai.core.ffi.Route
import dev.rootshell.kai.core.ffi.VoiceMode
import kotlinx.coroutines.launch

/**
 * The voice settings (design §3.4, §10.1; presented 2026-10-19): everyday choices first (who
 * decides when you speak, the output, the echo canceller or half duplex, the cues), the waits, the
 * playout reports and the two tails under Advanced (push-to-talk's tail must fit inside lmgw's
 * commit window, §7.5; the echo tail is §8.2's). Each value is the core's; a value it refuses is
 * said with the setting's label.
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
    Column(modifier.padding(horizontal = 16.dp), verticalArrangement = Arrangement.spacedBy(4.dp)) {
        SectionHeader(stringResource(R.string.set_voice_turns))
        SupportingText(stringResource(R.string.set_voice_turns_help))
        ChoiceRow(stringResource(R.string.voice_mode_thread), mode == VoiceMode.THREAD, stringResource(R.string.voice_mode_thread_help)) { mode = VoiceMode.THREAD }
        ChoiceRow(stringResource(R.string.voice_mode_automatic), mode == VoiceMode.AUTOMATIC, stringResource(R.string.voice_mode_automatic_help)) { mode = VoiceMode.AUTOMATIC }
        ChoiceRow(stringResource(R.string.voice_mode_ptt), mode == VoiceMode.PUSH_TO_TALK, stringResource(R.string.voice_mode_ptt_help)) { mode = VoiceMode.PUSH_TO_TALK }
        SectionHeader(stringResource(R.string.set_voice_output))
        SupportingText(stringResource(R.string.set_voice_output_help))
        ChoiceRow(stringResource(R.string.voice_route_speaker), route == Route.SPEAKER) { route = Route.SPEAKER }
        ChoiceRow(stringResource(R.string.voice_route_earpiece), route == Route.EARPIECE) { route = Route.EARPIECE }
        SectionHeader(stringResource(R.string.set_voice_echo))
        SupportingText(stringResource(R.string.voice_echo_note))
        ChoiceRow(stringResource(R.string.voice_echo_auto), echo == EchoMode.AUTO, stringResource(R.string.voice_echo_auto_help)) { echo = EchoMode.AUTO }
        ChoiceRow(stringResource(R.string.voice_echo_hardware), echo == EchoMode.HARDWARE, stringResource(R.string.voice_echo_hardware_help)) { echo = EchoMode.HARDWARE }
        ChoiceRow(stringResource(R.string.voice_echo_software), echo == EchoMode.SOFTWARE, stringResource(R.string.voice_echo_software_help)) { echo = EchoMode.SOFTWARE }
        ChoiceRow(stringResource(R.string.voice_echo_half_duplex), echo == EchoMode.HALF_DUPLEX, stringResource(R.string.voice_echo_half_duplex_help)) { echo = EchoMode.HALF_DUPLEX }
        SectionHeader(stringResource(R.string.set_voice_cues))
        SettingSwitch(stringResource(R.string.set_earcon), stringResource(R.string.set_earcon_help), earcon) { earcon = it }
        SettingSwitch(stringResource(R.string.set_haptics), stringResource(R.string.set_haptics_help), haptics) { haptics = it }
        AdvancedGroup {
            SettingNumber(R.string.set_connect, R.string.set_connect_help, R.string.unit_s, connect) { connect = it }
            SettingNumber(R.string.set_idle, R.string.set_idle_help, R.string.unit_s, idle) { idle = it }
            SettingNumber(R.string.set_report, R.string.set_report_help, R.string.unit_ms, report) { report = it }
            SettingNumber(R.string.set_ptt_tail, R.string.set_ptt_tail_help, R.string.unit_ms, tail, optional = true, placeholder = R.string.set_none) { tail = it }
            SettingNumber(R.string.set_echo_tail, R.string.set_echo_tail_help, R.string.unit_ms, echoTail, optional = true, placeholder = R.string.set_lmgw_value) { echoTail = it }
        }
        Button(onClick = {
            val fields = listOf(
                Triple(SettingKey.CONNECT_TIMEOUT, connect, false), Triple(SettingKey.IDLE_END, idle, false),
                Triple(SettingKey.PLAYOUT_REPORT, report, false), Triple(SettingKey.PTT_TAIL, tail, true),
                Triple(SettingKey.ECHO_TAIL, echoTail, true),
            )
            val bad = fields.firstOrNull { !WholeNumber.valid(it.second, it.third) }
            if (bad != null) {
                said = app.getString(R.string.set_needs_number, app.getString(bad.first.label))
                return@Button
            }
            scope.launch {
                said = try {
                    val s = app.core.settings()
                    s.voice.mode = mode
                    s.voice.connectTimeoutS = WholeNumber.parse(connect)
                    s.voice.idleEndS = WholeNumber.parse(idle)
                    s.voice.playoutReportMs = WholeNumber.parse(report)
                    s.voice.pttTailMs = WholeNumber.parseOptional(tail)
                    s.voice.route = route
                    s.voice.echo = echo
                    s.voice.echoTailMs = WholeNumber.parseOptional(echoTail)
                    s.voice.earcon = earcon
                    s.voice.haptics = haptics
                    app.core.setSettings(s)
                    // A changed echo setting rebuilds the factory now, not at the next call's start.
                    app.calls.refreshFactory()
                    app.getString(R.string.voice_saved)
                } catch (e: CoreException) {
                    refusalWords(app, e)
                }
            }
        }) { Text(stringResource(R.string.save)) }
        said?.let { Text(it, style = MaterialTheme.typography.bodyMedium) }
    }
}
