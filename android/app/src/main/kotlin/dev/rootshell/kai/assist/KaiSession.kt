package dev.rootshell.kai.assist

import android.Manifest
import android.content.BroadcastReceiver
import android.content.Context
import android.content.Intent
import android.content.IntentFilter
import android.content.pm.PackageManager
import android.content.res.Configuration
import android.graphics.Bitmap
import android.os.Build
import android.os.Bundle
import android.os.Handler
import android.os.HandlerThread
import android.os.Looper
import android.os.SystemClock
import android.service.voice.VoiceInteractionSession
import android.util.Log
import android.view.FrameMetrics
import android.view.View
import android.view.Window
import androidx.activity.OnBackPressedDispatcher
import androidx.activity.OnBackPressedDispatcherOwner
import androidx.activity.compose.LocalOnBackPressedDispatcherOwner
import androidx.activity.setViewTreeOnBackPressedDispatcherOwner
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.ui.platform.ComposeView
import androidx.core.content.ContextCompat
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
import dev.rootshell.kai.KaiApplication
import dev.rootshell.kai.MainActivity
import dev.rootshell.kai.R
import dev.rootshell.kai.TalkActivity
import dev.rootshell.kai.core.ffi.CallStart
import dev.rootshell.kai.ui.Layout
import dev.rootshell.kai.ui.Routes
import dev.rootshell.kai.ui.approvals.ApprovalRules
import dev.rootshell.kai.ui.layoutForWidthDp
import dev.rootshell.kai.ui.theme.KaiTheme
import dev.rootshell.kai.voice.active
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import kotlinx.coroutines.withTimeoutOrNull

/**
 * The assistant overlay's session (design §9.2): a bottom sheet over whatever the owner was doing,
 * that starts a call by the Kai folder at once and hands it to the app on Expand. Every decision is
 * [OverlayModel]'s (the application's one instance); the call is the application's (the core, the
 * engine, the controller, §9.3), so it outlives the sheet: a dismissal after the owner's first turn
 * leaves it going on in the background with its notification, and Expand moves it into the app
 * without a reconnect.
 *
 * - **`onPrepareShow`** makes sure libwebrtc's factory exists (a process the system restarted since
 *   `onReady`) and warms a connection to lmgw while the offer is built (§7.2), unless a call runs.
 * - **`onShow`** starts the call, shows a running one, or shows why not (locked: §9.6; the
 *   microphone not allowed). The microphone type joins Kai's service from here (the assistant's
 *   while-in-use exemption, §8.6; WP0 (b)).
 * - **`onHide`** is a dismissal unless Expand, Keyboard, Unlock or Open Kai caused it; the core
 *   decides what it does to the call the overlay started (§7.7).
 * - **The first frame** after each invocation is logged (tag `kai`) with its time from `onShow`
 *   and from `onPrepareShow`, warm or cold, and the window's width class: the numbers WP4's
 *   "within the budget" (§10.4: under 300 ms warm) is read from.
 *
 * - **The approval cards** (§11): while the sheet shows unlocked it shows them, and tells the core
 *   so (`Core::set_overlay`), so no notification asks for the same call; hidden or locked, it does
 *   not.
 * - **Back** goes through the session's own `OnBackPressedDispatcher`, wired to the window's
 *   `OnBackInvokedDispatcher` (API 33+), so the sheet follows the back gesture (predictive back)
 *   and a completed one dismisses it; the session's `onBackPressed` (the key, older Android) asks
 *   the same dispatcher, whose fallback hides.
 *
 * A ComposeView outside an activity needs its view-tree owners set by hand, so the session is its
 * own `LifecycleOwner`, `SavedStateRegistryOwner`, `ViewModelStoreOwner` and
 * `OnBackPressedDispatcherOwner` (as WP0's spike, with the back owner added).
 * Screen content is never read (`setDisabledShowContext`, [KaiInteractionService]); any that comes
 * anyway is logged and dropped.
 */
class KaiSession(context: Context) :
    VoiceInteractionSession(context),
    LifecycleOwner,
    SavedStateRegistryOwner,
    ViewModelStoreOwner,
    OnBackPressedDispatcherOwner {

    private val app = context.applicationContext as KaiApplication
    private val overlay get() = app.overlay
    private val registry = LifecycleRegistry(this)
    private val savedState = SavedStateRegistryController.create(this)
    private val store = ViewModelStore()
    override val lifecycle: Lifecycle get() = registry
    override val savedStateRegistry: SavedStateRegistry get() = savedState.savedStateRegistry
    override val viewModelStore: ViewModelStore get() = store

    /** Back with no handler enabled (none is, outside the sheet) hides, as the session's default. */
    private val back = OnBackPressedDispatcher { hide() }
    override val onBackPressedDispatcher: OnBackPressedDispatcher get() = back

    private val main = Handler(Looper.getMainLooper())

    /** The model's facts the sheet draws (the hub's state it collects itself). */
    private val locked = mutableStateOf(false)
    private val blocked = mutableStateOf<Blocked?>(null)
    private val starting = mutableStateOf(false)
    private val started = mutableStateOf<ULong?>(null)

    /** Expand found no path that opened the app: the sheet says so and keeps the call. */
    private val expandError = mutableStateOf<String?>(null)

    private var lockWatch: BroadcastReceiver? = null
    private val frames = FirstFrame()

    /** This session shows (the model's facts are about the session that shows last). */
    private var shown = false

    init {
        savedState.performAttach()
    }

    override fun onCreate() {
        super.onCreate()
        savedState.performRestore(null)
        registry.currentState = Lifecycle.State.CREATED
    }

    override fun onPrepareShow(args: Bundle?, showFlags: Int) {
        super.onPrepareShow(args, showFlags)
        frames.prepared(args)
        // The factory and the warm connection ahead of the offer (§7.2); never under a running call.
        if (!app.hub.voice.value.active()) app.calls.prepare()
        registry.currentState = Lifecycle.State.RESUMED
    }

    override fun onCreateContentView(): View {
        val view = ComposeView(context)
        view.setViewTreeLifecycleOwner(this)
        view.setViewTreeSavedStateRegistryOwner(this)
        view.setViewTreeViewModelStoreOwner(this)
        view.setViewTreeOnBackPressedDispatcherOwner(this)
        window?.window?.decorView?.let {
            it.setViewTreeLifecycleOwner(this)
            it.setViewTreeSavedStateRegistryOwner(this)
            it.setViewTreeViewModelStoreOwner(this)
            it.setViewTreeOnBackPressedDispatcherOwner(this)
        }
        // The back gesture's progress reaches the sheet's PredictiveBackHandler (API 33+).
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU) {
            window?.window?.let { back.setOnBackInvokedDispatcher(it.onBackInvokedDispatcher) }
        }
        view.setContent {
            CompositionLocalProvider(LocalOnBackPressedDispatcherOwner provides this) {
                KaiTheme(colours = app.core.colours()) { Sheet() }
            }
        }
        frames.attach(window?.window)
        return view
    }

    override fun onShow(args: Bundle?, showFlags: Int) {
        super.onShow(args, showFlags)
        frames.shown(showFlags, context.resources.configuration.screenWidthDp)
        shown = true
        watchTheLock()
        act(overlay.shown(facts()))
    }

    override fun onHide() {
        super.onHide()
        unwatchTheLock()
        if (registry.currentState.isAtLeast(Lifecycle.State.CREATED)) registry.currentState = Lifecycle.State.CREATED
        hidden()
    }

    override fun onDestroy() {
        unwatchTheLock()
        hidden()
        registry.currentState = Lifecycle.State.DESTROYED
        store.clear()
        frames.close()
        super.onDestroy()
    }

    /** The back key (and back on Android before 13): the sheet's handler, else hide. */
    override fun onBackPressed() = back.onBackPressed()

    override fun onHandleAssist(state: AssistState) {
        Log.w(TAG, "assist: the system handed over screen content despite setDisabledShowContext; it is dropped (design §9.7)")
    }

    override fun onHandleScreenshot(screenshot: Bitmap?) {
        if (screenshot != null) Log.w(TAG, "assist: the system handed over a screenshot despite setDisabledShowContext; it is dropped (design §9.7)")
    }

    // ---------------------------------------------------------------- the model's decisions

    private fun facts() = ShowFacts(
        locked = app.notifier.locked(),
        microphone = ContextCompat.checkSelfPermission(context, Manifest.permission.RECORD_AUDIO) == PackageManager.PERMISSION_GRANTED,
        call = app.hub.voice.value,
    )

    private fun act(show: Show) {
        Log.i(TAG, "assist: $show")
        sync()
        if (show is Show.Start) start(show.start)
    }

    private fun start(s: CallStart) {
        app.scope.launch {
            val result = app.calls.start(s)
            main.post {
                overlay.answered(result)?.let { call ->
                    Log.i(TAG, "assist: the overlay was dismissed while call $call started; the core decides")
                    app.core.voice.dismissed(call)
                }
                sync()
            }
        }
    }

    private fun hidden() {
        if (!shown) return
        shown = false
        overlay.hidden()?.let { call ->
            Log.i(TAG, "assist: the overlay was dismissed; the core decides on call $call")
            app.core.voice.dismissed(call)
        }
        sync()
    }

    /** A fold or an unfold while the overlay shows: the lock state is read again, the sheet redraws. */
    override fun onConfigurationChanged(newConfig: Configuration) {
        super.onConfigurationChanged(newConfig)
        if (shown) sync()
    }

    private fun sync() {
        locked.value = app.notifier.locked()
        blocked.value = overlay.blocked
        starting.value = overlay.starting
        started.value = overlay.started
        // The cards show (and count as seen) only on the unlocked sheet (§11, §9.6).
        app.core.setOverlay(ApprovalRules.overlaySees(shown, locked.value))
    }

    /**
     * Hands the call (or the owner) over to the app, then hides: no dismissal (§9.2). The overlay
     * lets go of the call only once a path has opened the app ([expandThrough]); if none did, the
     * sheet stays with its error and the overlay keeps the call.
     */
    private fun handOver(into: Into, target: Class<*>) {
        expandError.value = null
        if (target == MainActivity::class.java) {
            app.hub.pendingRoute.value = Routes.MAIN
            if (into == Into.CONVERSATION) app.hub.openConversation.value = true
        }
        val intent = Intent(context, target)
            .addFlags(Intent.FLAG_ACTIVITY_NEW_TASK or Intent.FLAG_ACTIVITY_CLEAR_TOP or Intent.FLAG_ACTIVITY_SINGLE_TOP)
        app.scope.launch {
            val used = expandThrough(EXPAND_PATHS) { open(intent, it) }
            main.post {
                if (used != null) {
                    Log.i(TAG, "assist: into the app by $used (${intent.component?.shortClassName})")
                    overlay.handover()
                    hide()
                } else {
                    Log.w(TAG, "assist: no path opened the app; the overlay keeps the call")
                    app.hub.pendingRoute.value = null
                    app.hub.openConversation.value = false
                    expandError.value = context.getString(R.string.overlay_expand_failed)
                }
            }
        }
    }

    /**
     * `intent` by one path (WP0 (a)'s seam: [EXPAND_PATHS]): true when the start did not throw and
     * the app showed within `lmgw.timeout_s`. Android 14 and later refuse a start from the
     * background without throwing, so only the app becoming visible says it opened.
     */
    private suspend fun open(intent: Intent, path: ExpandPath): Boolean {
        val threw = withContext(Dispatchers.Main) {
            runCatching {
                when (path) {
                    ExpandPath.ASSISTANT_ACTIVITY -> startAssistantActivity(intent)
                    ExpandPath.START_ACTIVITY -> context.startActivity(intent)
                }
            }.exceptionOrNull()
        }
        if (threw != null) {
            Log.w(TAG, "assist: $path did not open the app: ${threw.javaClass.simpleName}: ${threw.message}")
            return false
        }
        val timeoutMs = app.core.settings().lmgw.timeoutS.toLong() * 1000
        val shown = withTimeoutOrNull(timeoutMs) { app.hub.visibleState.first { it } } == true
        if (!shown) Log.w(TAG, "assist: $path returned, but the app did not show within ${timeoutMs / 1000} s")
        return shown
    }

    private val actions = object : OverlayActions {
        override fun dismiss() = hide()

        override fun expand() = handOver(Into.VOICE, MainActivity::class.java)

        override fun keyboard() = handOver(Into.CONVERSATION, MainActivity::class.java)

        override fun openKai(why: Blocked) = when (why) {
            // The voice screen asks for the microphone, then starts the call.
            Blocked.Microphone, Blocked.Locked -> handOver(Into.VOICE, TalkActivity::class.java)
            // The link's state, with its actions, on the voice screen; no call.
            is Blocked.Words -> handOver(Into.VOICE, MainActivity::class.java)
        }

        override fun unlock() = handOver(Into.VOICE, TalkActivity::class.java)

        override fun talk(start: CallStart) = act(overlay.talk(facts(), start))

        override fun end() {
            app.core.voice.end()
            hide()
        }
    }

    @Composable
    private fun Sheet() {
        val view by app.hub.voice.collectAsState()
        val captions by app.hub.captions.collectAsState()
        val approvals by app.hub.approvals.collectAsState()
        val face = face(locked.value, blocked.value, starting.value, view)
        OverlaySheet(app, face, view, captions, approvals, dismissalHint(view, started.value), expandError.value, actions)
    }

    // ---------------------------------------------------------------- the lock (§9.6)

    /** While the overlay shows: an unlock lets the invocation go on, a lock hides its words. */
    private fun watchTheLock() {
        if (lockWatch != null) return
        val receiver = object : BroadcastReceiver() {
            override fun onReceive(c: Context, intent: Intent) {
                when (lockStep(intent.action)) {
                    // The screen went off: hidden, so the dismissal goes through onHide and the
                    // core's rule decides (a call after the owner's first turn goes on).
                    LockStep.Hide -> hide()
                    LockStep.Sync -> {
                        sync()
                        if (!app.notifier.locked()) overlay.unlocked(facts())?.let { act(it) }
                    }
                }
            }
        }
        val filter = IntentFilter().apply {
            addAction(Intent.ACTION_USER_PRESENT)
            addAction(Intent.ACTION_SCREEN_ON)
            addAction(Intent.ACTION_SCREEN_OFF)
        }
        ContextCompat.registerReceiver(context, receiver, filter, ContextCompat.RECEIVER_NOT_EXPORTED)
        lockWatch = receiver
    }

    private fun unwatchTheLock() {
        lockWatch?.let { runCatching { context.unregisterReceiver(it) } }
        lockWatch = null
    }

    private companion object {
        const val TAG = "kai"
    }
}

/**
 * The overlay's first frame after an invocation (design §10.4: under 300 ms warm), from the
 * window's frame metrics: logged once per show with its time after `onShow` and after
 * `onPrepareShow`, after the press where the system's arguments carry its time, warm or cold (a
 * process younger than [COLD_MS]), and the window's width class (compact: the cover screen, wide:
 * the inner one).
 */
private class FirstFrame {
    private var thread: HandlerThread? = null
    private var attached: Window? = null

    @Volatile private var prepareNanos = 0L
    @Volatile private var showNanos = 0L
    @Volatile private var pressElapsed = 0L
    @Volatile private var ageMs = 0L
    @Volatile private var flags = 0
    @Volatile private var widthDp = 0
    @Volatile private var reported = true

    fun prepared(args: Bundle?) {
        prepareNanos = System.nanoTime()
        showNanos = 0L
        ageMs = KaiInteractionService.processAgeMs()
        // SystemUI's assist arguments carry the invocation's time on the elapsed clock; logged when
        // it is there and plausible.
        pressElapsed = args?.getLong(INVOCATION_TIME_MS, 0L) ?: 0L
        reported = false
    }

    fun shown(showFlags: Int, widthDp: Int) {
        showNanos = System.nanoTime()
        flags = showFlags
        this.widthDp = widthDp
    }

    fun attach(window: Window?) {
        if (window == null || attached === window) return
        val t = thread ?: HandlerThread("kai-overlay-frames").also {
            it.start()
            thread = it
        }
        try {
            window.addOnFrameMetricsAvailableListener(listener, Handler(t.looper))
            attached = window
        } catch (e: Exception) {
            Log.i(TAG, "assist: the overlay's frame metrics are not available (${e.javaClass.simpleName}); its first frame is not timed")
        }
    }

    fun close() {
        runCatching { attached?.removeOnFrameMetricsAvailableListener(listener) }
        attached = null
        thread?.quitSafely()
        thread = null
    }

    private val listener = Window.OnFrameMetricsAvailableListener { _, metrics, _ ->
        if (reported) return@OnFrameMetricsAvailableListener
        val vsync = metrics.getMetric(FrameMetrics.INTENDED_VSYNC_TIMESTAMP)
        if (vsync < prepareNanos) return@OnFrameMetricsAvailableListener
        reported = true
        val end = vsync + metrics.getMetric(FrameMetrics.TOTAL_DURATION)
        fun ms(from: Long) = if (from == 0L) "?" else ((end - from) / 1_000_000).toString()
        val press = pressElapsed.takeIf { it > 0 }?.let {
            val endElapsed = (end + SystemClock.elapsedRealtimeNanos() - System.nanoTime()) / 1_000_000
            (endElapsed - it).takeIf { d -> d in 0..PLAUSIBLE_MS }
        }
        val temperature = if (ageMs < COLD_MS) "cold" else "warm"
        val screen = if (layoutForWidthDp(widthDp) == Layout.Compact) "compact" else "wide"
        Log.i(
            TAG,
            "assist: the overlay's first frame ${ms(showNanos)} ms after onShow, ${ms(prepareNanos)} ms after onPrepareShow" +
                (press?.let { ", $it ms after the invocation" } ?: "") +
                " ($temperature, $screen $widthDp dp, show flags 0x${flags.toString(16)}, importance ${KaiInteractionService.importance()})",
        )
    }

    private companion object {
        const val TAG = "kai"
        const val INVOCATION_TIME_MS = "invocation_time_ms"

        /** A show in a process younger than this counts as cold (as WP0's spike). */
        const val COLD_MS = 5000L

        /** An invocation time further back than this is not the press of this show. */
        const val PLAUSIBLE_MS = 20_000L
    }
}
