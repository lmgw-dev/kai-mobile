package dev.rootshell.kai.spike

import android.Manifest
import android.app.Activity
import android.content.Intent
import android.content.pm.PackageManager
import android.os.Bundle
import android.os.SystemClock
import android.view.Choreographer
import android.view.ViewTreeObserver
import androidx.activity.ComponentActivity
import androidx.activity.compose.BackHandler
import androidx.activity.compose.LocalActivity
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.compose.setContent
import androidx.activity.enableEdgeToEdge
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.safeDrawingPadding
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.Button
import androidx.compose.material3.Card
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.unit.dp
import dev.rootshell.kai.spike.points.EchoScreen
import dev.rootshell.kai.spike.points.FfiScreen
import dev.rootshell.kai.spike.points.HeadsetScreen
import dev.rootshell.kai.spike.points.InvocationScreen
import dev.rootshell.kai.spike.points.MicTypeScreen
import dev.rootshell.kai.spike.points.Recreations
import dev.rootshell.kai.spike.points.ScreensScreen
import dev.rootshell.kai.spike.points.SelfCheckScreen
import dev.rootshell.kai.spike.probe.FfiBench
import dev.rootshell.kai.spike.probe.SelfCheck
import dev.rootshell.kai.spike.ui.Point
import dev.rootshell.kai.ui.theme.KaiTheme
import kotlin.concurrent.thread

/**
 * The spike's one activity: the list of points and one page per point. Fully resizable and
 * recreated by a fold (n); the open page survives that. It also logs how it was reached: the
 * overlay's Expand (a), ASSIST or VOICE_COMMAND (design §9.1, §9.4).
 *
 * `--es autorun o` (or `e`) runs (o)'s self-check (or (e) in this process) at start, for a check
 * over adb without touching the phone.
 */
class SpikeActivity : ComponentActivity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        enableEdgeToEdge()
        super.onCreate(savedInstanceState)
        Recreations.created(this, savedInstanceState != null)
        arrived(intent, true)
        setContent { KaiTheme { SpikeRoot() } }
        if (savedInstanceState == null) autorun(intent.getStringExtra(EXTRA_AUTORUN))
    }

    override fun onNewIntent(intent: Intent) {
        super.onNewIntent(intent)
        arrived(intent, false)
    }

    override fun onStart() {
        super.onStart()
        SpikeApp.visibleActivities++
    }

    override fun onStop() {
        SpikeApp.visibleActivities--
        super.onStop()
    }

    override fun onDestroy() {
        Recreations.destroyed(this)
        super.onDestroy()
    }

    /** Logs an Expand from the overlay (with the time to this activity's first frame) or an ASSIST. */
    private fun arrived(intent: Intent, created: Boolean) {
        val how = intent.getStringExtra(EXTRA_EXPAND)
        if (how != null) {
            val t0 = intent.getLongExtra(EXTRA_EXPAND_T0, 0)
            val arrivedMs = SystemClock.elapsedRealtime() - t0
            intent.removeExtra(EXTRA_EXPAND)
            val decor = window.decorView
            val listener = object : ViewTreeObserver.OnDrawListener {
                var done = false
                override fun onDraw() {
                    if (done) return
                    done = true
                    Choreographer.getInstance().postFrameCallback {
                        SpikeLog.event(
                            "a", "expand_arrived",
                            mapOf("how" to how, "created" to created, "arrived_ms" to arrivedMs,
                                "first_frame_ms" to SystemClock.elapsedRealtime() - t0, "posture" to Device.posture(this@SpikeActivity)),
                        )
                    }
                }
            }
            decor.viewTreeObserver.addOnDrawListener(listener)
        }
        if (intent.action == Intent.ACTION_ASSIST || intent.action == Intent.ACTION_VOICE_COMMAND) {
            SpikeLog.event("a", "activity_intent", mapOf("action" to intent.action, "created" to created,
                "extras" to intent.extras?.keySet()?.toList()))
        }
    }

    private fun autorun(which: String?) {
        when (which) {
            "o" -> thread(name = "spike-autorun") { SelfCheck.run(applicationContext) }
            "e" -> thread(name = "spike-autorun") {
                SpikeLog.event("e", "result", mapOf("result" to FfiBench.run(applicationContext, false, "autorun")))
            }
        }
    }

    companion object {
        const val EXTRA_EXPAND = "spike_expand"
        const val EXTRA_EXPAND_T0 = "spike_expand_t0"
        const val EXTRA_AUTORUN = "autorun"
    }
}

@Composable
private fun SpikeRoot() {
    var open by rememberSaveable { mutableStateOf<String?>(null) }
    val point = Point.of(open)
    BackHandler(enabled = point != null) { open = null }
    Surface(Modifier.fillMaxSize()) {
        val back = { open = null }
        when (point) {
            null -> Home { open = it.key }
            Point.A -> InvocationScreen(back)
            Point.B -> MicTypeScreen(back)
            Point.D -> EchoScreen(back)
            Point.E -> FfiScreen(back)
            Point.I -> HeadsetScreen(back)
            Point.N -> ScreensScreen(back)
            Point.O -> SelfCheckScreen(back)
        }
    }
}

private val permissions = arrayOf(
    Manifest.permission.RECORD_AUDIO,
    Manifest.permission.POST_NOTIFICATIONS,
    Manifest.permission.BLUETOOTH_CONNECT,
)

@Composable
private fun Home(onOpen: (Point) -> Unit) {
    val context = LocalContext.current
    var tick by remember { mutableIntStateOf(0) }
    val launcher = rememberLauncherForActivityResult(ActivityResultContracts.RequestMultiplePermissions()) { result ->
        SpikeLog.event("app", "permissions", result.mapValues { it.value })
        tick++
    }
    Column(
        Modifier.fillMaxSize().safeDrawingPadding().verticalScroll(rememberScrollState()).padding(16.dp),
        verticalArrangement = Arrangement.spacedBy(12.dp),
    ) {
        Text("Kai WP0 device spike", style = MaterialTheme.typography.headlineSmall)
        Text(
            "Each page says what you do; every measurement goes to the screen and to ${SpikeLog.path()}, " +
                "which `just spike-log` pulls. Fill spikes/wp0/RESULTS.md from it.",
            style = MaterialTheme.typography.bodyMedium,
        )
        Card(Modifier.fillMaxWidth().widthIn(max = 840.dp)) {
            Column(Modifier.padding(16.dp), verticalArrangement = Arrangement.spacedBy(6.dp)) {
                Text("Permissions", style = MaterialTheme.typography.titleSmall)
                @Suppress("UNUSED_EXPRESSION") tick
                permissions.forEach {
                    Text("${it.substringAfterLast('.')}: ${if (granted(context, it)) "granted" else "not granted"}")
                }
                Text(
                    "The microphone for (b), (d), (i), (n); notifications for (b) and (o); Bluetooth names for (i) only " +
                        "(grant it after a first (i) run without it, to see what it changes).",
                    style = MaterialTheme.typography.bodySmall,
                )
                Button(onClick = { launcher.launch(permissions) }) { Text("Grant") }
            }
        }
        Point.entries.forEach { p ->
            Card(Modifier.fillMaxWidth().widthIn(max = 840.dp).clickable { onOpen(p) }) {
                Column(Modifier.padding(16.dp)) {
                    Text(p.title, style = MaterialTheme.typography.titleMedium)
                    Text(p.question, style = MaterialTheme.typography.bodySmall)
                }
            }
        }
        Text(
            "Waiting for lmgw's calls route and feed: (c), (f), (g), (h), (j), (k), (l), (m).",
            style = MaterialTheme.typography.bodySmall,
        )
    }
}

/** The activity of a composition, for the pages that need it. */
@Composable
fun activity(): Activity = checkNotNull(LocalActivity.current) { "no activity" }

/** Whether `permission` is granted; notifications need no grant below Android 13. */
fun granted(context: android.content.Context, permission: String): Boolean =
    (permission == Manifest.permission.POST_NOTIFICATIONS && android.os.Build.VERSION.SDK_INT < 33) ||
        context.checkSelfPermission(permission) == PackageManager.PERMISSION_GRANTED
