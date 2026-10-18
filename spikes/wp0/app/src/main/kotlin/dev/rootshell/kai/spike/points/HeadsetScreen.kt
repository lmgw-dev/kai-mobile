package dev.rootshell.kai.spike.points

import android.Manifest
import android.content.Context
import android.content.pm.PackageManager
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.ExperimentalLayoutApi
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.material3.Button
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.unit.dp
import dev.rootshell.kai.spike.SpikeLog
import dev.rootshell.kai.spike.audio.HeadsetTrial
import dev.rootshell.kai.spike.ui.Point
import dev.rootshell.kai.spike.ui.PointPage
import dev.rootshell.kai.spike.ui.Results
import dev.rootshell.kai.spike.ui.Steps
import dev.rootshell.kai.spike.ui.Toggle
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext

/** The headset trial outlives the activity (a fold must not end it). */
private object Headset {
    @Volatile private var trial: HeadsetTrial? = null
    fun get(context: Context) = trial ?: HeadsetTrial(context.applicationContext).also { trial = it }
}

@OptIn(ExperimentalLayoutApi::class)
@Composable
fun HeadsetScreen(onBack: () -> Unit) {
    val context = LocalContext.current
    val trial = Headset.get(context)
    val scope = rememberCoroutineScope()
    var voiceUsage by remember { mutableStateOf(true) }
    var communication by remember { mutableStateOf(true) }
    var buttonsOn by remember { mutableStateOf(trial.running) }
    var hfp by remember { mutableStateOf(emptyList<String>()) }
    var busy by remember { mutableStateOf(false) }
    val entries by SpikeLog.entries.collectAsState()
    val buttons = entries.filter { it.point == "i" && it.event == "button" }.takeLast(20).map { it.text }
    PointPage(Point.I, onBack) {
        Steps(
            "Wired headset: plug it in, Run, then press its button once, twice quickly, and hold it. Each key that reaches the session is listed below with its device.",
            "Bluetooth headset: connect it, Run, press its call/play button the same ways. Repeat with the toggles changed (media usage, normal mode) if nothing arrives.",
            "Bluetooth audio: tap Route and record, then speak for 6 seconds: the SCO setup time, the sample rates and the bandwidth (narrowband or wideband) come out; the WAV stays in wp0/hfp/.",
            "Bluetooth names: run once without BLUETOOTH_CONNECT, then grant it on the list and run again; the device lines show whether names need it.",
        )
        Toggle("Play as a call (USAGE_VOICE_COMMUNICATION), else as media", voiceUsage) { voiceUsage = it }
        Toggle("MODE_IN_COMMUNICATION during the test", communication) { communication = it }
        FlowRow(horizontalArrangement = Arrangement.spacedBy(8.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
            Button(enabled = !buttonsOn, onClick = { trial.startButtons(voiceUsage, communication); buttonsOn = true }) { Text("Run: the button test") }
            OutlinedButton(enabled = buttonsOn, onClick = { trial.stopButtons(); buttonsOn = false }) { Text("Stop") }
            OutlinedButton(enabled = !busy, onClick = {
                if (context.checkSelfPermission(Manifest.permission.RECORD_AUDIO) != PackageManager.PERMISSION_GRANTED) {
                    hfp = listOf("grant the microphone first")
                    return@OutlinedButton
                }
                busy = true
                hfp = listOf("recording 6 s: speak now…")
                scope.launch {
                    val r = withContext(Dispatchers.IO) { runCatching { trial.hfp() } }
                    hfp = r.getOrNull()?.let { h ->
                        listOf(
                            "device ${h.device}, accepted ${h.accepted}" + (h.error?.let { ", $it" } ?: ""),
                            "SCO: communication device changed after ${h.changedMs} ms, recording routed after ${h.routedMs} ms, first speech after ${h.firstSpeechMs} ms",
                            "rates: client ${h.clientRate} Hz, device format ${h.deviceFormatRate} Hz, device offers ${h.deviceRates}",
                            "bandwidth: ${h.bandwidth} (energy above 4 kHz ${h.shareAbove4k?.let { "%.3f".format(it) }}, above 7 kHz ${h.shareAbove7k?.let { "%.3f".format(it) }})",
                        )
                    } ?: listOf("failed: ${r.exceptionOrNull()}")
                    busy = false
                }
            }) { Text("Bluetooth: route and record 6 s") }
        }
        Text("BLUETOOTH_CONNECT: ${if (context.checkSelfPermission(Manifest.permission.BLUETOOTH_CONNECT) == PackageManager.PERMISSION_GRANTED) "granted" else "not granted"}")
        Results("Keys the session received (newest last)", buttons)
        Results("Bluetooth audio", hfp)
    }
}
