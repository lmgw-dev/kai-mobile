package dev.rootshell.kai.spike.points

import android.app.Activity
import android.hardware.display.DisplayManager
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.ExperimentalLayoutApi
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.material3.Button
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.Text
import androidx.compose.material3.adaptive.currentWindowAdaptiveInfoV2
import androidx.compose.runtime.Composable
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.ui.platform.LocalConfiguration
import androidx.compose.ui.unit.dp
import dev.rootshell.kai.spike.SpikeLog
import dev.rootshell.kai.spike.WindowFacts
import dev.rootshell.kai.spike.activity
import dev.rootshell.kai.spike.assist.SpikeInteractionService
import dev.rootshell.kai.spike.service.SpikeService
import dev.rootshell.kai.spike.ui.Point
import dev.rootshell.kai.spike.ui.PointPage
import dev.rootshell.kai.spike.ui.Results
import dev.rootshell.kai.spike.ui.Steps
import kotlinx.coroutines.flow.MutableStateFlow

/**
 * (n)'s record of the activity's lives in this process: every creation and destruction with the
 * window it had and the service's capture counters at that moment, so a fold mid-capture shows
 * whether audio was lost.
 */
object Recreations {
    data class Entry(val wallMs: Long, val event: String, val window: String, val capture: String?)

    val entries = MutableStateFlow<List<Entry>>(emptyList())

    private fun capture(): Pair<String?, Map<String, Any?>> {
        val c = SpikeService.states.value.capture
        if (c == null || !c.running) return null to emptyMap()
        return "capture ${c.durationMs / 1000} s, lost ${"%.0f".format(c.lostMs)} ms, max gap ${"%.0f".format(c.maxGapMs)} ms, gaps ${c.gapsOver100}" to
            mapOf("capture" to c.toMap())
    }

    fun created(a: Activity, restored: Boolean) {
        val facts = WindowFacts.of(a)
        val (text, fields) = capture()
        SpikeLog.event("n", "activity_created", facts.toMap() + fields + mapOf("restored" to restored))
        entries.value = entries.value + Entry(System.currentTimeMillis(), if (restored) "recreated" else "created", facts.toString(), text)
    }

    fun destroyed(a: Activity) {
        val (text, fields) = capture()
        SpikeLog.event("n", "activity_destroyed", fields + mapOf("changing_configurations" to a.isChangingConfigurations, "finishing" to a.isFinishing))
        entries.value = entries.value + Entry(
            System.currentTimeMillis(),
            "destroyed (${if (a.isChangingConfigurations) "configuration change" else if (a.isFinishing) "finishing" else "by the system"})",
            "", text,
        )
    }
}

@OptIn(ExperimentalLayoutApi::class)
@Composable
fun ScreensScreen(onBack: () -> Unit) {
    val a = activity()
    val configuration = LocalConfiguration.current
    val facts = WindowFacts.of(a)
    val max = WindowFacts.maximum(a.windowManager, a.resources.configuration, a.resources.displayMetrics.density)
    val adaptive = currentWindowAdaptiveInfoV2()
    val displays = a.getSystemService(DisplayManager::class.java).displays.map {
        "display ${it.displayId} \"${it.name}\": ${it.mode.physicalWidth}×${it.mode.physicalHeight} px, state ${it.state}, rotation ${it.rotation}"
    }
    val recreations by Recreations.entries.collectAsState()
    val service by SpikeService.states.collectAsState()
    val metrics = mapOf(
        "window" to facts.toMap(), "maximum" to max.toMap(),
        "configuration" to mapOf(
            "screen_width_dp" to configuration.screenWidthDp, "screen_height_dp" to configuration.screenHeightDp,
            "smallest_width_dp" to configuration.smallestScreenWidthDp, "density_dpi" to configuration.densityDpi,
            "orientation" to configuration.orientation,
        ),
        "adaptive" to mapOf(
            "min_width_dp" to adaptive.windowSizeClass.minWidthDp, "min_height_dp" to adaptive.windowSizeClass.minHeightDp,
            "tabletop" to adaptive.windowPosture.isTabletop, "hinges" to adaptive.windowPosture.hingeList.size,
        ),
        "displays" to displays,
    )
    PointPage(Point.N, onBack) {
        Steps(
            "Folded (cover screen): Run. Unfold: Run again. The log keeps both screens' dp and size classes.",
            "Start a capture, then fold and unfold a few times, also mid-sentence while you speak. Each fold recreates this screen; the list below shows the capture's counters at every recreation (lost should stay near 0, max gap under 100 ms). Stop the capture.",
            "The overlay on both screens: Show the overlay (or press the side button), fold or unfold while it shows; the log's a.overlay_layout lines give its size on each screen.",
            "One UI: Settings → Display → Continue apps on cover screen → Kai WP0 spike on, then off: fold with this screen open each time. The list shows whether the activity came back on the cover screen.",
        )
        FlowRow(horizontalArrangement = Arrangement.spacedBy(8.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
            Button(onClick = { SpikeLog.event("n", "metrics", metrics) }) { Text("Run: record this screen") }
            OutlinedButton(onClick = {
                SpikeService.start(a, "fold_test")
                SpikeService.micOn(a, "fold_test")
            }) { Text("Start a capture") }
            OutlinedButton(onClick = { SpikeService.micOff("fold_test") }) { Text("Stop the capture") }
            OutlinedButton(onClick = {
                val vis = SpikeInteractionService.instance
                if (vis == null) SpikeLog.event("n", "show_overlay", mapOf("ok" to false, "why" to "the spike is not the active assistant"))
                else vis.showFromApp("screens")
            }) { Text("Show the overlay") }
        }
        Results(
            "This window",
            listOf(
                "current: $facts", "maximum: $max",
                "configuration: ${configuration.screenWidthDp}×${configuration.screenHeightDp} dp, smallest ${configuration.smallestScreenWidthDp} dp, ${configuration.densityDpi} dpi",
                "adaptive: min ${adaptive.windowSizeClass.minWidthDp}×${adaptive.windowSizeClass.minHeightDp} dp, tabletop ${adaptive.windowPosture.isTabletop}, hinges ${adaptive.windowPosture.hingeList.size}",
            ) + displays + listOf(
                "service: ${if (service.running) service.types else "not running"}" +
                    (service.capture?.takeIf { it.running }?.let { " · capture ${it.durationMs / 1000} s, lost ${"%.0f".format(it.lostMs)} ms" } ?: ""),
            ),
        )
        Results(
            "This activity's lives in this process (newest last)",
            recreations.map { e -> "${e.event} ${e.window}" + (e.capture?.let { " · $it" } ?: "") },
        )
    }
}
