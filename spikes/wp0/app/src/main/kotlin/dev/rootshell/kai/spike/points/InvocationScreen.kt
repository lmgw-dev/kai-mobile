package dev.rootshell.kai.spike.points

import android.app.role.RoleManager
import android.content.ComponentName
import android.content.Context
import android.content.Intent
import android.content.res.Resources
import android.os.Process
import android.os.SystemClock
import android.provider.Settings
import android.service.voice.VoiceInteractionService
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.ExperimentalLayoutApi
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.material3.Button
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableLongStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.LifecycleResumeEffect
import dev.rootshell.kai.spike.Prefs
import dev.rootshell.kai.spike.SpikeLog
import dev.rootshell.kai.spike.assist.SpikeInteractionService
import dev.rootshell.kai.spike.assist.SpikeSession
import dev.rootshell.kai.spike.ui.Point
import dev.rootshell.kai.spike.ui.PointPage
import dev.rootshell.kai.spike.ui.Results
import dev.rootshell.kai.spike.ui.Steps
import dev.rootshell.kai.spike.ui.Toggle

/** What the phone says about the assistant setup (a). */
private fun assistStatus(context: Context): Map<String, Any?> {
    val roles = context.getSystemService(RoleManager::class.java)
    val cr = context.contentResolver
    fun secure(key: String) = runCatching { Settings.Secure.getString(cr, key) }.getOrElse { "unreadable: ${it.javaClass.simpleName}" }
    fun global(key: String) = runCatching { Settings.Global.getString(cr, key) }.getOrElse { "unreadable: ${it.javaClass.simpleName}" }
    val powerRes = Resources.getSystem().getIdentifier("config_longPressOnPowerBehavior", "integer", "android")
    return mapOf(
        "role_available" to roles.isRoleAvailable(RoleManager.ROLE_ASSISTANT),
        "role_held" to roles.isRoleHeld(RoleManager.ROLE_ASSISTANT),
        "active_service" to runCatching {
            VoiceInteractionService.isActiveService(context, ComponentName(context, SpikeInteractionService::class.java))
        }.getOrElse { "unreadable: ${it.javaClass.simpleName}" },
        "service_in_process" to (SpikeInteractionService.instance != null),
        "secure_assistant" to secure("assistant"),
        "secure_voice_interaction_service" to secure("voice_interaction_service"),
        "secure_voice_recognition_service" to secure("voice_recognition_service"),
        "config_longPressOnPowerBehavior" to if (powerRes != 0) runCatching { Resources.getSystem().getInteger(powerRes) }.getOrNull() else null,
        "global_power_button_long_press" to global("power_button_long_press"),
        "disable_show_context_pref" to Prefs.disableShowContext(context),
        "disabled_show_context_now" to SpikeInteractionService.instance?.disabledShowContext,
    )
}

@OptIn(ExperimentalLayoutApi::class)
@Composable
fun InvocationScreen(onBack: () -> Unit) {
    val context = LocalContext.current
    var status by remember { mutableStateOf(assistStatus(context)) }
    var disable by remember { mutableStateOf(Prefs.disableShowContext(context)) }
    var micOnShow by remember { mutableStateOf(Prefs.micOnShow(context)) }
    var roleAsked by remember { mutableLongStateOf(0L) }
    var roleResult by remember { mutableStateOf<String?>(null) }
    val invocations by SpikeSession.invocationList.collectAsState()
    LifecycleResumeEffect(Unit) {
        status = assistStatus(context)
        onPauseOrDispose {}
    }
    val roleLauncher = rememberLauncherForActivityResult(ActivityResultContracts.StartActivityForResult()) { r ->
        val ms = SystemClock.elapsedRealtime() - roleAsked
        roleResult = "result ${r.resultCode} after $ms ms: did a dialog show?"
        SpikeLog.event("a", "role_request_result", mapOf("result_code" to r.resultCode, "after_ms" to ms,
            "role_held" to context.getSystemService(RoleManager::class.java).isRoleHeld(RoleManager.ROLE_ASSISTANT)))
        status = assistStatus(context)
    }
    PointPage(Point.A, onBack) {
        Steps(
            "Run: the setup as the phone reports it is logged. Then Request the role and answer whether a dialog showed.",
            "If no dialog: Open the assistant setting and choose Kai WP0 spike as the digital assistant app (Samsung/One UI: Settings → Apps → Choose default apps → Digital assistant app).",
            "Samsung foldables (One UI): Settings → Advanced features → Side button → Press and hold → note whether Kai WP0 spike is offered; choose it.",
            "Warm runs: with the spike in the background, press and hold the side button, 10 times on the cover screen and 10 times unfolded. The overlay shows its own first-frame time; close it each time.",
            "Cold runs: Kill the process here (or adb shell am force-stop dev.rootshell.kai.spike), then press and hold at once. If the log calls the run warm, the system restarted the process before the press: that is a finding too.",
            "In the overlay, try both Expand buttons: each logs whether the app opened and its first frame.",
            "setDisabledShowContext: switch the toggle, Kill the process (onReady applies it), invoke: the log shows show_flags and whether onHandleAssist/onHandleScreenshot came.",
            "LineageOS: whether Settings offers the power button's long press for the assistant; the status shows config_longPressOnPowerBehavior and power_button_long_press (else adb shell settings put global power_button_long_press 5).",
        )
        FlowRow(horizontalArrangement = Arrangement.spacedBy(8.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
            Button(onClick = {
                status = assistStatus(context)
                SpikeLog.event("a", "status", status)
            }) { Text("Run: check the setup") }
            OutlinedButton(onClick = {
                roleAsked = SystemClock.elapsedRealtime()
                SpikeLog.event("a", "role_request")
                roleLauncher.launch(context.getSystemService(RoleManager::class.java).createRequestRoleIntent(RoleManager.ROLE_ASSISTANT))
            }) { Text("Request the role") }
            OutlinedButton(onClick = {
                SpikeLog.event("a", "open_settings", mapOf("action" to Settings.ACTION_VOICE_INPUT_SETTINGS))
                context.startActivity(Intent(Settings.ACTION_VOICE_INPUT_SETTINGS))
            }) { Text("Open the assistant setting") }
            OutlinedButton(onClick = {
                context.startActivity(Intent(Settings.ACTION_MANAGE_DEFAULT_APPS_SETTINGS))
            }) { Text("Open default apps") }
        }
        roleResult?.let { r ->
            Text(r)
            FlowRow(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                OutlinedButton(onClick = { SpikeLog.event("a", "role_dialog", mapOf("shown" to true)); roleResult = null }) { Text("A dialog showed") }
                OutlinedButton(onClick = { SpikeLog.event("a", "role_dialog", mapOf("shown" to false)); roleResult = null }) { Text("No dialog") }
            }
        }
        Results("Setup", status.map { "${it.key}: ${it.value}" })
        Toggle("setDisabledShowContext(SHOW_WITH_ASSIST | SHOW_WITH_SCREENSHOT) in onReady", disable) {
            disable = it
            Prefs.setDisableShowContext(context, it)
            SpikeLog.event("a", "toggle", mapOf("disable_show_context" to it))
        }
        Toggle("Add the microphone type whenever the overlay shows (b)", micOnShow) {
            micOnShow = it
            Prefs.setMicOnShow(context, it)
            SpikeLog.event("a", "toggle", mapOf("mic_on_show" to it))
        }
        FlowRow(horizontalArrangement = Arrangement.spacedBy(8.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
            OutlinedButton(onClick = {
                val vis = SpikeInteractionService.instance
                if (vis == null) SpikeLog.event("a", "show_from_app", mapOf("ok" to false, "why" to "the spike is not the active assistant"))
                else vis.showFromApp("app_button")
            }) { Text("Show the overlay now") }
            OutlinedButton(onClick = {
                SpikeLog.event("a", "kill_process")
                SpikeLog.flush()
                Process.killProcess(Process.myPid())
            }) { Text("Kill the process (cold run)") }
        }
        Results("Invocations (newest last; all in the log as a.first_frame)", invocations.map { it.toString() })
    }
}
