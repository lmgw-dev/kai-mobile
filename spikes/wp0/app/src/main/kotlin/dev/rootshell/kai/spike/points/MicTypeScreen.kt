package dev.rootshell.kai.spike.points

import android.os.Handler
import android.os.Looper
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.ExperimentalLayoutApi
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.material3.Button
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.unit.dp
import dev.rootshell.kai.spike.Prefs
import dev.rootshell.kai.spike.SpikeLog
import dev.rootshell.kai.spike.service.SpikeService
import dev.rootshell.kai.spike.ui.Point
import dev.rootshell.kai.spike.ui.PointPage
import dev.rootshell.kai.spike.ui.Results
import dev.rootshell.kai.spike.ui.Steps
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch

@OptIn(ExperimentalLayoutApi::class)
@Composable
fun MicTypeScreen(onBack: () -> Unit) {
    val context = LocalContext.current
    val app = context.applicationContext
    val scope = rememberCoroutineScope()
    val state by SpikeService.states.collectAsState()
    var host by remember { mutableStateOf(Prefs.pingHost(context)) }
    var minutes by remember { mutableStateOf(Prefs.captureMinutes(context).toString()) }
    PointPage(Point.B, onBack) {
        Steps(
            "Grant the microphone and notifications (the list's Permissions card). Enter a LAN host for the ping (a machine at home, e.g. 192.0.2.10, or host:port for TCP).",
            "From a visible activity: Run. The service starts as specialUse, then this screen adds the microphone type (watch for the microphone indicator), and the capture and the ping start.",
            "Press the side button once (screen off) and leave the phone for 10 minutes. Wake and unlock it, open the spike: the summary below and capture_tick lines show lost audio, gaps, silencing and the ping with the screen off.",
            "Look at the notification on the lock screen before unlocking (it should show the capture with Mute and End). End drops the microphone type; check that the indicator goes out.",
            "From the assistant session: with the service running (Start the service), go Home, press and hold the side button, tap Mic on (b) in the overlay, then dismiss the overlay (swipe down or back). Then the 10 minutes with the screen off as above. Repeat with the service stopped first (the session starts it).",
            "Control: Add in 10 s, then go Home at once. Android should refuse the type from the background unless the active-assistant exemption covers it: the log's b.foreground line says which.",
        )
        OutlinedTextField(host, { host = it; Prefs.setPingHost(context, it) }, Modifier.fillMaxWidth(),
            label = { Text("Ping host (empty: no ping)") }, singleLine = true)
        OutlinedTextField(minutes, { v -> minutes = v; v.toIntOrNull()?.let { Prefs.setCaptureMinutes(context, it) } }, Modifier.fillMaxWidth(),
            label = { Text("Stop the capture after minutes (0: only by hand)") }, singleLine = true)
        FlowRow(horizontalArrangement = Arrangement.spacedBy(8.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
            Button(onClick = {
                scope.launch {
                    SpikeService.start(context, "activity")
                    var waited = 0
                    while (SpikeService.instance == null && waited < 3000) {
                        delay(50)
                        waited += 50
                    }
                    // A type change on a running service, as at a call's start (design §8.6).
                    delay(500)
                    SpikeService.micOn(context, "activity")
                }
            }) { Text("Run from this screen") }
            OutlinedButton(onClick = { SpikeService.start(context, "activity") }) { Text("Start the service") }
            OutlinedButton(onClick = { SpikeService.micOn(context, "activity") }) { Text("Add the microphone type") }
            OutlinedButton(onClick = {
                if (SpikeService.instance == null) SpikeService.start(context, "activity")
                SpikeLog.event("b", "background_add_scheduled", mapOf("in_s" to 10))
                Handler(Looper.getMainLooper()).postDelayed({ SpikeService.micOn(app, "background_10s") }, 10_000)
            }) { Text("Add in 10 s (go Home now)") }
            OutlinedButton(onClick = { SpikeService.micOff("activity") }) { Text("Drop the microphone type") }
            OutlinedButton(onClick = { SpikeService.stop(context, "activity") }) { Text("Stop the service") }
        }
        val c = state.capture
        val p = state.ping
        Results(
            "Service and capture",
            listOfNotNull(
                "service: ${if (state.running) "running, types ${state.types}" else "not running"}",
                "last step: ${state.lastStep}",
                c?.let {
                    "capture (${if (it.running) "running" else "stopped"}, from ${it.origin}): ${it.durationMs / 1000} s, " +
                        "lost ${"%.0f".format(it.lostMs)} ms, max gap ${"%.0f".format(it.maxGapMs)} ms, gaps over 100 ms ${it.gapsOver100}, " +
                        "level ${"%.1f".format(it.lastRmsDb)} dBFS, silenced ${it.silencedEver}, screen off ${it.screenOffMs / 1000} s" +
                        (it.error?.let { e -> ", error: $e" } ?: "")
                },
                p?.let { "ping ${it.target} (${it.mode}): sent ${it.sent}, lost ${it.lost}" + (it.error?.let { e -> ", $e" } ?: "") },
                p?.let { "  screen on: ${it.screenOn}" },
                p?.let { "  screen off: ${it.screenOff}" },
            ),
        )
    }
}
