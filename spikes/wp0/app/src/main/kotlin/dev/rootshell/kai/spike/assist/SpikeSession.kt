package dev.rootshell.kai.spike.assist

import android.content.Context
import android.content.Intent
import android.graphics.Bitmap
import android.os.Bundle
import android.os.Handler
import android.os.HandlerThread
import android.os.Looper
import android.os.SystemClock
import android.service.voice.VoiceInteractionSession
import android.view.FrameMetrics
import android.view.View
import android.view.Window
import androidx.compose.foundation.Canvas
import androidx.compose.foundation.clickable
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ExperimentalLayoutApi
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.navigationBarsPadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.Button
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.platform.ComposeView
import androidx.compose.ui.unit.dp
import androidx.lifecycle.Lifecycle
import androidx.lifecycle.LifecycleOwner
import androidx.lifecycle.LifecycleRegistry
import androidx.lifecycle.ViewModelStore
import androidx.lifecycle.ViewModelStoreOwner
import androidx.lifecycle.setViewTreeLifecycleOwner
import androidx.lifecycle.setViewTreeViewModelStoreOwner
import androidx.savedstate.SavedStateRegistry
import androidx.savedstate.SavedStateRegistryController
import androidx.savedstate.SavedStateRegistryOwner
import androidx.savedstate.setViewTreeSavedStateRegistryOwner
import dev.rootshell.kai.spike.Prefs
import dev.rootshell.kai.spike.SpikeActivity
import dev.rootshell.kai.spike.SpikeApp
import dev.rootshell.kai.spike.SpikeLog
import dev.rootshell.kai.spike.WindowFacts
import dev.rootshell.kai.spike.r1
import dev.rootshell.kai.spike.service.SpikeService
import dev.rootshell.kai.spike.webrtc.WebRtc
import dev.rootshell.kai.ui.theme.KaiTheme
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow

/**
 * The overlay (design §9.2; WP0 (a), (b), (n)): a ComposeView in the session's window, with the
 * view-tree owners set by hand. Every show measures the time to the overlay's first frame: from
 * `onPrepareShow`, from `onShow`, and from the press itself when the system's arguments carry its
 * time. Each one is logged as `a.first_frame` with the process's age (cold or warm), the screen
 * (cover or inner) and the arguments as they came.
 */
class SpikeSession(context: Context) :
    VoiceInteractionSession(context),
    LifecycleOwner,
    SavedStateRegistryOwner,
    ViewModelStoreOwner {

    private val registry = LifecycleRegistry(this)
    private val savedState = SavedStateRegistryController.create(this)
    private val store = ViewModelStore()
    override val lifecycle: Lifecycle get() = registry
    override val savedStateRegistry: SavedStateRegistry get() = savedState.savedStateRegistry
    override val viewModelStore: ViewModelStore get() = store

    init {
        savedState.performAttach()
    }

    /** One show's marks; reported once, at its first frame. */
    private class Mark(
        val prepareNanos: Long,
        val processAgeMs: Long,
        val args: Bundle?,
        val flags: Int,
        val webrtcReady: Boolean,
    ) {
        @Volatile var showNanos = 0L
        @Volatile var drawNanos = 0L
        @Volatile var reported = false
    }

    private val main = Handler(Looper.getMainLooper())
    private val frames = HandlerThread("spike-frames").apply { start() }
    private val frameHandler = Handler(frames.looper)
    @Volatile private var mark: Mark? = null
    private var contentViewNanos = 0L
    private var metricsAttached = false
    private var status by mutableStateOf("connecting · no lmgw in WP0")
    private var frameText by mutableStateOf("measuring the first frame…")

    override fun onCreate() {
        super.onCreate()
        savedState.performRestore(null)
        registry.currentState = Lifecycle.State.CREATED
    }

    override fun onPrepareShow(args: Bundle?, showFlags: Int) {
        super.onPrepareShow(args, showFlags)
        val ready = WebRtc.factory != null
        mark = Mark(System.nanoTime(), SpikeApp.processAgeMs(), args, showFlags, ready)
        // The factory exists unless the system restarted the process since onReady (design §9.2).
        if (!ready) WebRtc.prepare(context)
        registry.currentState = Lifecycle.State.RESUMED
        showing = true
        frameText = "measuring the first frame…"
        visible.value = true
    }

    override fun onCreateContentView(): View {
        contentViewNanos = System.nanoTime()
        val view = ComposeView(context).apply {
            setContent { KaiTheme { Overlay() } }
        }
        view.setViewTreeLifecycleOwner(this)
        view.setViewTreeSavedStateRegistryOwner(this)
        view.setViewTreeViewModelStoreOwner(this)
        window?.window?.decorView?.let {
            it.setViewTreeLifecycleOwner(this)
            it.setViewTreeSavedStateRegistryOwner(this)
            it.setViewTreeViewModelStoreOwner(this)
        }
        view.viewTreeObserver.addOnDrawListener {
            val m = mark
            if (m != null && m.drawNanos == 0L) m.drawNanos = System.nanoTime()
        }
        view.addOnLayoutChangeListener { v, l, t, r, b, ol, ot, orr, ob ->
            if (r - l != orr - ol || b - t != ob - ot) {
                val wm = window?.window?.windowManager ?: return@addOnLayoutChangeListener
                val facts = WindowFacts.of(wm, v.resources.configuration, v.resources.displayMetrics.density)
                SpikeLog.event("a", "overlay_layout", facts.toMap() + mapOf("view_w_px" to r - l, "view_h_px" to b - t))
            }
        }
        attachFrameMetrics()
        return view
    }

    override fun onShow(args: Bundle?, showFlags: Int) {
        super.onShow(args, showFlags)
        val m = mark ?: return
        m.showNanos = System.nanoTime()
        attachFrameMetrics()
        if (Prefs.micOnShow(context)) SpikeService.micOn(context, "session_on_show")
        // Without frame metrics, the first draw after the show stands in.
        main.postDelayed({
            if (!m.reported && m.drawNanos != 0L) {
                m.reported = true
                report(m, m.drawNanos, "first_draw", null)
            }
        }, 1500)
    }

    override fun onHandleAssist(state: AssistState) {
        SpikeLog.event(
            "a", "handle_assist",
            mapOf(
                "data" to (state.assistData != null), "structure" to (state.assistStructure != null),
                "content" to (state.assistContent != null), "index" to state.index, "count" to state.count,
            ),
        )
    }

    override fun onHandleScreenshot(screenshot: Bitmap?) {
        SpikeLog.event("a", "handle_screenshot", mapOf("bitmap" to (screenshot != null)))
    }

    override fun onHide() {
        super.onHide()
        showing = false
        visible.value = false
        if (registry.currentState.isAtLeast(Lifecycle.State.CREATED)) registry.currentState = Lifecycle.State.CREATED
        SpikeLog.event("a", "hide", mapOf("process_age_ms" to SpikeApp.processAgeMs()))
    }

    override fun onDestroy() {
        showing = false
        visible.value = false
        registry.currentState = Lifecycle.State.DESTROYED
        store.clear()
        frames.quitSafely()
        SpikeLog.event("a", "session_destroyed")
        super.onDestroy()
    }

    private fun attachFrameMetrics() {
        if (metricsAttached) return
        val w = window?.window ?: return
        try {
            w.addOnFrameMetricsAvailableListener(frameListener, frameHandler)
            metricsAttached = true
        } catch (e: Exception) {
            SpikeLog.event("a", "frame_metrics_unavailable", mapOf("error" to e.javaClass.name, "message" to e.message))
        }
    }

    private val frameListener = Window.OnFrameMetricsAvailableListener { _, metrics, _ ->
        val m = mark
        if (m == null || m.reported) return@OnFrameMetricsAvailableListener
        val vsync = metrics.getMetric(FrameMetrics.INTENDED_VSYNC_TIMESTAMP)
        if (vsync < m.prepareNanos) return@OnFrameMetricsAvailableListener
        m.reported = true
        val end = vsync + metrics.getMetric(FrameMetrics.TOTAL_DURATION)
        val first = metrics.getMetric(FrameMetrics.FIRST_DRAW_FRAME) == 1L
        main.post { report(m, end, "frame_metrics", first) }
    }

    /** Logs one show's first frame, which ended at `frameNanos` (System.nanoTime's clock). */
    private fun report(m: Mark, frameNanos: Long, via: String, firstDraw: Boolean?) {
        // nanoTime and uptimeMillis share CLOCK_MONOTONIC; elapsedRealtime adds the time asleep.
        val offsetNanos = SystemClock.elapsedRealtimeNanos() - System.nanoTime()
        val frameElapsedMs = (frameNanos + offsetNanos) / 1e6
        val frameUptimeMs = frameNanos / 1e6
        val args = m.args
        val dump = buildMap<String, Any?> {
            args?.keySet()?.forEach { k -> put(k, @Suppress("DEPRECATION") args.get(k)?.toString()) }
        }
        // A time in the arguments, read on whichever clock puts it shortly before the frame.
        var refKey: String? = null
        var refMs: Double? = null
        var refBase: String? = null
        for (key in listOf("invocation_time_ms", Intent.EXTRA_TIME, SpikeInteractionService.EXTRA_T0)) {
            @Suppress("DEPRECATION")
            val v = (args?.get(key) as? Number)?.toDouble() ?: continue
            val e = frameElapsedMs - v
            val u = frameUptimeMs - v
            val picked = when {
                e in 0.0..20_000.0 -> e to "elapsedRealtime"
                u in 0.0..20_000.0 -> u to "uptimeMillis"
                else -> null
            } ?: continue
            refKey = key
            refMs = picked.first.r1()
            refBase = picked.second
            break
        }
        fun since(nanos: Long) = if (nanos == 0L) null else ((frameNanos - nanos) / 1e6).r1()
        val wm = window?.window?.windowManager
        val facts = wm?.let { WindowFacts.of(it, context.resources.configuration, context.resources.displayMetrics.density) }
        val newSession = newSessionNanos.takeIf { it >= m.prepareNanos - 10_000_000_000L } ?: 0L
        val visAge = SpikeInteractionService.readyElapsed.takeIf { it > 0 }?.let { SystemClock.elapsedRealtime() - it }
        val cold = m.processAgeMs < COLD_MS
        val fields = mapOf(
            "source" to (args?.getString(SpikeInteractionService.EXTRA_SOURCE) ?: "system"),
            "show_flags" to m.flags, "args" to dump, "via" to via, "first_draw" to firstDraw,
            "prepare_to_frame_ms" to since(m.prepareNanos), "show_to_frame_ms" to since(m.showNanos),
            "new_session_to_frame_ms" to since(newSession), "content_view_to_frame_ms" to since(contentViewNanos),
            "ref_key" to refKey, "ref_to_frame_ms" to refMs, "ref_clock" to refBase,
            "process_age_at_prepare_ms" to m.processAgeMs, "cold" to cold, "vis_ready_age_ms" to visAge,
            "sessions_in_process" to sessionsInProcess, "webrtc_ready_at_prepare" to m.webrtcReady,
            "importance" to SpikeInteractionService.importance(), "posture" to facts?.posture,
            "window" to facts?.toMap(),
        )
        SpikeLog.event("a", "first_frame", fields)
        val line = Invocation(
            System.currentTimeMillis(), fields["source"] as String, cold, facts?.posture ?: "?",
            since(m.prepareNanos), since(m.showNanos), refMs, refKey,
        )
        invocations.value = (invocations.value + line).takeLast(50)
        frameText = line.toString()
        contentViewNanos = 0L
    }

    private fun expand(viaAssistantActivity: Boolean) {
        val how = if (viaAssistantActivity) "startAssistantActivity" else "startActivity"
        val intent = Intent(context, SpikeActivity::class.java)
            .putExtra(SpikeActivity.EXTRA_EXPAND, how)
            .putExtra(SpikeActivity.EXTRA_EXPAND_T0, SystemClock.elapsedRealtime())
            .addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)
        val outcome = runCatching {
            if (viaAssistantActivity) startAssistantActivity(intent) else context.startActivity(intent)
        }
        SpikeLog.event(
            "a", "expand_call",
            mapOf("how" to how, "ok" to outcome.isSuccess, "error" to outcome.exceptionOrNull()?.let { "${it.javaClass.name}: ${it.message}" }),
        )
        hide()
    }

    @OptIn(ExperimentalLayoutApi::class)
    @Composable
    private fun Overlay() {
        val service by SpikeService.states.collectAsState()
        val outside = remember { MutableInteractionSource() }
        val inside = remember { MutableInteractionSource() }
        Box(
            Modifier.fillMaxSize().clickable(interactionSource = outside, indication = null) { hide() },
            contentAlignment = Alignment.BottomCenter,
        ) {
            // The sheet: the cover screen's width, Material's maximum sheet width on the inner one
            // (design §9.2, §10.6).
            Surface(
                Modifier.widthIn(max = 640.dp).fillMaxWidth()
                    .clickable(interactionSource = inside, indication = null) {},
                shape = RoundedCornerShape(topStart = 28.dp, topEnd = 28.dp),
                tonalElevation = 3.dp,
            ) {
                Column(
                    Modifier.navigationBarsPadding().padding(24.dp),
                    verticalArrangement = Arrangement.spacedBy(12.dp),
                    horizontalAlignment = Alignment.CenterHorizontally,
                ) {
                    val ring = MaterialTheme.colorScheme.primary
                    Canvas(Modifier.size(96.dp)) {
                        drawCircle(ring, radius = size.minDimension / 2 - 6.dp.toPx(), style = Stroke(6.dp.toPx()))
                    }
                    Text("Kai WP0 spike", style = MaterialTheme.typography.titleMedium)
                    Text(status, style = MaterialTheme.typography.bodyMedium)
                    Text(frameText, style = MaterialTheme.typography.bodySmall)
                    Text(
                        "service: ${if (service.running) service.types else "not running"}" +
                            (service.capture?.takeIf { it.running }?.let { " · capturing ${it.durationMs / 1000} s" } ?: ""),
                        style = MaterialTheme.typography.bodySmall,
                    )
                    FlowRow(horizontalArrangement = Arrangement.spacedBy(8.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
                        Button(onClick = { SpikeService.micOn(context, "session") }) { Text("Mic on (b)") }
                        OutlinedButton(onClick = { SpikeService.micOff("session") }) { Text("Mic off") }
                        OutlinedButton(onClick = { expand(true) }) { Text("Expand: assistant activity") }
                        OutlinedButton(onClick = { expand(false) }) { Text("Expand: startActivity") }
                        OutlinedButton(onClick = { hide() }) { Text("Close") }
                    }
                }
            }
        }
    }

    /** One invocation as (a)'s screen lists it. */
    data class Invocation(
        val wallMs: Long,
        val source: String,
        val cold: Boolean,
        val posture: String,
        val prepareToFrameMs: Double?,
        val showToFrameMs: Double?,
        val refToFrameMs: Double?,
        val refKey: String?,
    ) {
        override fun toString(): String =
            "${if (cold) "cold" else "warm"} · $posture · $source: first frame ${prepareToFrameMs} ms after onPrepareShow, " +
                "${showToFrameMs} ms after onShow" + (refToFrameMs?.let { ", $it ms after the invocation ($refKey)" } ?: "")
    }

    companion object {
        /** A show in a process younger than this counts as cold. */
        const val COLD_MS = 5000L

        @Volatile var showing = false
        @Volatile var newSessionNanos = 0L
        @Volatile var sessionsInProcess = 0

        val visible = MutableStateFlow(false)
        val invocations = MutableStateFlow<List<Invocation>>(emptyList())
        val invocationList: StateFlow<List<Invocation>> get() = invocations
    }
}
