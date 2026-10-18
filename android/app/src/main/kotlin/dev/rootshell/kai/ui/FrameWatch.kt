package dev.rootshell.kai.ui

import android.app.Activity
import android.content.Context
import android.content.ContextWrapper
import android.os.Handler
import android.os.HandlerThread
import android.util.Log
import android.view.FrameMetrics
import android.view.View
import android.view.Window
import androidx.compose.foundation.gestures.ScrollableState
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.snapshotFlow
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.platform.LocalView
import androidx.compose.ui.platform.LocalWindowInfo
import androidx.compose.ui.window.DialogWindowProvider
import java.util.Locale
import kotlin.math.max

/**
 * One scroll's frames against the display's budget (design §10.4: "no frame over the display's
 * budget"), pure so the JVM tests run it. Each frame brings its total time and its deadline
 * (`FrameMetrics.DEADLINE`, the display's frame interval at its refresh rate then, so a variable
 * refresh rate is judged frame by frame).
 */
class FrameTally {
    var frames = 0
        private set
    var over = 0
        private set
    var worstNs = 0L
        private set
    var deadlineNs = 0L
        private set

    fun add(totalNs: Long, deadlineNs: Long) {
        frames++
        if (deadlineNs in 1..<totalNs) over++
        worstNs = max(worstNs, totalNs)
        if (deadlineNs > 0) this.deadlineNs = deadlineNs
    }

    /** The log line: frames, those over their deadline, the worst, the last deadline. */
    fun summary(what: String, where: String): String =
        "$what: scrolled $frames frames ($where), $over over the display's deadline " +
            "(${ms(deadlineNs)} ms), the worst ${ms(worstNs)} ms"

    private fun ms(ns: Long) = String.format(Locale.ROOT, "%.1f", ns / 1_000_000.0)
}

/** One thread for every window's frame metrics. */
private object FrameThread {
    val handler: Handler by lazy { Handler(HandlerThread("kai-frames").also { it.start() }.looper) }
}

/** The window `view` draws in: a dialog's (the conversation's sheet) or the activity's. */
private fun View.hostWindow(): Window? {
    var p = parent
    while (p != null) {
        if (p is DialogWindowProvider) return p.window
        p = p.parent
    }
    var c: Context? = context
    while (c is ContextWrapper) {
        if (c is Activity) return c.window
        c = c.baseContext
    }
    return null
}

/**
 * Measures each scroll of `state` (design §10.4's WP5 row): while it scrolls, the window's frame
 * metrics are tallied, and when it stops one line is logged (tag `kai`): the frames, those over the
 * display's deadline, the worst, the window's width. Only while a scroll runs is a listener
 * registered, so nothing is measured at rest. Read it with `adb logcat -s kai`; Perfetto
 * shows the same frames in detail.
 */
@Composable
fun ScrollFrameWatch(state: ScrollableState, what: String) {
    val view = LocalView.current
    val widthDp = with(LocalDensity.current) { LocalWindowInfo.current.containerSize.width.toDp().value.toInt() }
    LaunchedEffect(state, view) {
        var watching: Watching? = null
        try {
            snapshotFlow { state.isScrollInProgress }.collect { scrolling ->
                val w = watching
                if (scrolling && w == null) {
                    watching = Watching.start(view)
                } else if (!scrolling && w != null) {
                    w.stop()
                    watching = null
                    // The listener's thread may add a last frame meanwhile: the line reads what came.
                    FrameThread.handler.post {
                        if (w.tally.frames > 0) {
                            Log.i(TAG, w.tally.summary(what, "${layoutForWidthDp(widthDp).name.lowercase()} $widthDp dp"))
                        }
                    }
                }
            }
        } finally {
            // The screen left mid-scroll: the listener goes with it.
            watching?.stop()
        }
    }
}

/** One scroll's listener on its window. */
private class Watching(private val window: Window, val tally: FrameTally) {
    private val listener = Window.OnFrameMetricsAvailableListener { _, m, _ ->
        tally.add(m.getMetric(FrameMetrics.TOTAL_DURATION), m.getMetric(FrameMetrics.DEADLINE))
    }

    fun stop() {
        runCatching { window.removeOnFrameMetricsAvailableListener(listener) }
    }

    companion object {
        /** Null when the view has no window, or the window gives no frame metrics. */
        fun start(view: View): Watching? {
            val window = view.hostWindow() ?: return null
            val w = Watching(window, FrameTally())
            return runCatching { window.addOnFrameMetricsAvailableListener(w.listener, FrameThread.handler) }
                .map { w }
                .getOrNull()
        }
    }
}

private const val TAG = "kai"
