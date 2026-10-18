package dev.rootshell.kai

import android.app.Activity
import android.app.Application
import android.os.Bundle
import android.os.Handler
import android.os.Looper
import dev.rootshell.kai.app.CoreHub
import dev.rootshell.kai.app.HubSink
import dev.rootshell.kai.app.NetworkWatcher
import dev.rootshell.kai.app.Notifier
import dev.rootshell.kai.app.NotificationTags
import dev.rootshell.kai.app.WakeAlarm
import dev.rootshell.kai.assist.OverlayModel
import dev.rootshell.kai.core.CoreFacade
import dev.rootshell.kai.core.UniffiCoreFacade
import dev.rootshell.kai.core.ffi.CallPhase
import dev.rootshell.kai.core.ffi.ConnectionView
import dev.rootshell.kai.core.ffi.NotificationId
import dev.rootshell.kai.core.ffi.NotificationView
import dev.rootshell.kai.core.ffi.VoiceView
import dev.rootshell.kai.service.KaiService
import dev.rootshell.kai.voice.CallEngine
import dev.rootshell.kai.voice.VoiceController
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob

/**
 * Holds the one core for the process's life (design §3.1) and what follows it: the hub that collects
 * its updates, the notifications, the alarm, the default network's callback, Kai's service, the
 * voice call's libwebrtc side (it outlives the activity, which a fold recreates), and the app's
 * visibility (any activity started).
 */
class KaiApplication : Application() {
    val core: CoreFacade by lazy {
        // The notified ids outlive the process (§5.4): the core knows them before its first feed.
        val (approvals, replies) = NotificationTags.active(notifier.activeTags())
        UniffiCoreFacade.create(this, approvals, replies)
    }
    val notifier: Notifier by lazy { Notifier(this) }
    val hub: CoreHub by lazy { CoreHub(core, sink) }

    /** The call's audio around it (design §9.3): the microphone type, focus, MediaSession, route, earcons. */
    val voice: VoiceController by lazy { VoiceController(this) }

    /** The call's libwebrtc side (design §7.3); the call's rules are the core's. */
    val calls: CallEngine by lazy { CallEngine(this, core, voice) }

    /** The assistant overlay's decisions (design §9.2), for the process's life (sessions come and go). */
    val overlay = OverlayModel()
    /** The process's scope: work that must outlive a screen or a receiver's `onReceive`. */
    val scope = CoroutineScope(SupervisorJob() + Dispatchers.Default)
    private val main by lazy { Handler(Looper.getMainLooper()) }
    private var started = 0

    private val sink = object : HubSink {
        override fun notify(notification: NotificationView) = notifier.show(notification)
        override fun withdraw(id: NotificationId) = notifier.withdraw(id)
        override fun wakeAt(bootMs: ULong?) = WakeAlarm.set(this@KaiApplication, bootMs)
        override fun notice(text: String) = notifier.notice(text)

        override fun connection(previous: ConnectionView?, view: ConnectionView, visible: Boolean) {
            when {
                // Started from the visible app once enrolled, and whenever the app becomes visible
                // with a pairing and the service not running (§12.1).
                view.paired && visible && !KaiService.running -> KaiService.start(this@KaiApplication)
                !view.paired && previous?.paired == true -> KaiService.stop(this@KaiApplication)
            }
            val changed = previous == null || previous.state != view.state || previous.words != view.words ||
                previous.nextTryAtMs != view.nextTryAtMs
            // During a call the service's notification is the call's (design §8.6). On the main
            // thread, where the service adds and drops the microphone type, so the two never cross.
            if (view.paired && KaiService.running && changed) {
                main.post { if (!KaiService.inCall) notifier.showStatus(view) }
            }
        }

        override fun voice(previous: VoiceView?, view: VoiceView, visible: Boolean) {
            val live = view.phase in setOf(CallPhase.CONNECTING, CallPhase.PENDING, CallPhase.LIVE, CallPhase.ENDING)
            if (live) {
                val idle = core.settings().voice.idleEndS
                main.post { if (KaiService.inCall) notifier.showCall(view, idle) }
            }
            // An end the owner did not see: its words as a notice, with Take over where it offers
            // it (design §7.9, §12.3). The assistant overlay shows them while it is up.
            val justEnded = view.phase == CallPhase.ENDED && (previous?.phase != CallPhase.ENDED || previous.call != view.call)
            // Suppressed only while the sheet shows the end: the locked face shows no words (§9.6).
            val sheetShowsIt = overlay.showing && !notifier.locked()
            if (justEnded && !visible && !sheetShowsIt) view.end?.let { notifier.voiceNotice(it) }
        }
    }

    override fun onCreate() {
        super.onCreate()
        notifier.createChannels()
        notifier.watchTheLock()
        hub.start(scope)
        NetworkWatcher(this, core).start()
        registerActivityLifecycleCallbacks(
            object : ActivityLifecycleCallbacks {
                override fun onActivityStarted(activity: Activity) {
                    started++
                    hub.setVisible(true)
                }

                override fun onActivityStopped(activity: Activity) {
                    started = (started - 1).coerceAtLeast(0)
                    if (started == 0) hub.setVisible(false)
                }

                override fun onActivityCreated(activity: Activity, savedInstanceState: Bundle?) {}
                override fun onActivityResumed(activity: Activity) {}
                override fun onActivityPaused(activity: Activity) {}
                override fun onActivitySaveInstanceState(activity: Activity, outState: Bundle) {}
                override fun onActivityDestroyed(activity: Activity) {}
            },
        )
    }
}
