package dev.rootshell.kai.spike.points

import android.Manifest
import android.content.pm.PackageManager
import androidx.compose.material3.Button
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.platform.LocalContext
import dev.rootshell.kai.spike.SpikeLog
import dev.rootshell.kai.spike.audio.EchoTrial
import dev.rootshell.kai.spike.ui.Point
import dev.rootshell.kai.spike.ui.PointPage
import dev.rootshell.kai.spike.ui.Results
import dev.rootshell.kai.spike.ui.Steps
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext

@Composable
fun EchoScreen(onBack: () -> Unit) {
    val context = LocalContext.current
    val app = context.applicationContext
    val scope = rememberCoroutineScope()
    var running by remember { mutableStateOf(false) }
    var progress by remember { mutableStateOf("") }
    var results by remember { mutableStateOf(emptyList<String>()) }
    PointPage(Point.D, onBack) {
        Steps(
            "A quiet room, the phone on a table, the speaker free, no headset connected.",
            "Optional: your own speech clip instead of the generated one: adb push clip.wav /sdcard/Android/data/dev.rootshell.kai.spike/files/wp0/clip.wav (16-bit PCM; never committed).",
            "Run (about 2 minutes: three voice-call volumes × hardware and raw). Stay silent while it plays: every barge-in counted is a false one.",
            "Unfold (or fold) the phone and Run again. Your volume is restored afterwards.",
            "The recordings stay in wp0/echo/ (just spike-log pulls them) for a real VAD later.",
        )
        Button(enabled = !running, onClick = {
            if (context.checkSelfPermission(Manifest.permission.RECORD_AUDIO) != PackageManager.PERMISSION_GRANTED) {
                progress = "grant the microphone first (the list's Permissions card)"
                return@Button
            }
            running = true
            scope.launch {
                val r = withContext(Dispatchers.Default) {
                    runCatching { EchoTrial.run(app) { p -> progress = p } }
                }
                results = r.getOrNull()?.map { it.toString() } ?: listOf("failed: ${r.exceptionOrNull()}")
                r.exceptionOrNull()?.let { SpikeLog.event("d", "error", mapOf("error" to it.toString())) }
                progress = if (r.isSuccess) "done" else "failed"
                running = false
            }
        }) { Text(if (running) "Running…" else "Run") }
        if (progress.isNotEmpty()) Text(progress)
        Results(
            "False barge-ins (energy VAD: 200 ms above the noise floor + 15 dB, at least -50 dBFS)",
            results + listOf(
                "hardware = VOICE_COMMUNICATION + AcousticEchoCanceler; raw = UNPROCESSED/VOICE_RECOGNITION, then an NLMS " +
                    "filter as a software stand-in (not WebRTC's AEC3, which needs the clip through libwebrtc's playout: (c)).",
            ),
        )
    }
}
