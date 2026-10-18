package dev.rootshell.kai.spike.points

import androidx.compose.material3.Button
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.platform.LocalContext
import dev.rootshell.kai.spike.probe.SelfCheck
import dev.rootshell.kai.spike.ui.Point
import dev.rootshell.kai.spike.ui.PointPage
import dev.rootshell.kai.spike.ui.Results
import dev.rootshell.kai.spike.ui.Steps
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext

@Composable
fun SelfCheckScreen(onBack: () -> Unit) {
    val context = LocalContext.current
    val scope = rememberCoroutineScope()
    var lines by remember { mutableStateOf(emptyList<String>()) }
    var running by remember { mutableStateOf(false) }
    PointPage(Point.O, onBack) {
        Steps(
            "Grant notifications on the list's Permissions card. For the overlay item, make the spike the digital assistant first ((a)).",
            "Run. Every item says pass, FAIL or skip, with its time; the Android version is recorded for M15.",
            "On a LineageOS device it runs the same way (or: adb shell am start -n dev.rootshell.kai.spike/.SpikeActivity --es autorun o, then just spike-log).",
        )
        Button(enabled = !running, onClick = {
            running = true
            scope.launch {
                lines = withContext(Dispatchers.IO) { SelfCheck.run(context.applicationContext) }.map { it.toString() }
                running = false
            }
        }) { Text(if (running) "Running…" else "Run the self-check") }
        Results("Self-check", lines)
    }
}
