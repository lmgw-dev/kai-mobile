package dev.rootshell.kai.app

import android.app.AlarmManager
import android.app.PendingIntent
import android.content.BroadcastReceiver
import android.content.Context
import android.content.Intent
import android.util.Log
import dev.rootshell.kai.KaiApplication
import dev.rootshell.kai.core.ffi.CoreException
import dev.rootshell.kai.core.ffi.Decided
import dev.rootshell.kai.core.ffi.NotificationKind
import dev.rootshell.kai.service.KaiService
import kotlinx.coroutines.launch
import kotlinx.coroutines.withTimeoutOrNull

/**
 * A notification's actions (design §8.6, §11, §12.3): a call's Mute, Unmute, End and Take over
 * reach the core's `Voice`; Approve and Decline call `Core::decide`, which
 * POSTs from this process (Approve needs an unlocked phone: the action asks for authentication);
 * a dismissed approval stays dismissed across reconnects; a dismissed or tapped reply is gone.
 *
 * A decision may start the process: the core holds it until the open set is known (the first
 * `caught_up`, at most `lmgw.timeout_s`), so Kai's service is started first to keep the process
 * alive that long (a notification's action may start a foreground service from the background).
 * With the service running the receiver finishes at once; without it, `goAsync` holds the process
 * until the core answered.
 */
class NotificationActionReceiver : BroadcastReceiver() {
    override fun onReceive(context: Context, intent: Intent) {
        val app = context.applicationContext as KaiApplication
        val core = app.core
        val tag = intent.getStringExtra(EXTRA_TAG)
        val id = NotificationTags.parse(tag)
        // A cleared notification is never posted again at the lock's next change.
        if (intent.action == ACTION_DISMISSED) app.notifier.gone(tag)
        when (intent.action) {
            // The call notification's actions (design §8.6): Mute and End work from the lock screen.
            ACTION_CALL_MUTE -> core.voice.setMuted(true)
            ACTION_CALL_UNMUTE -> core.voice.setMuted(false)
            ACTION_CALL_END -> core.voice.end()
            ACTION_CALL_TAKE_OVER -> core.voice.takeOver()
            ACTION_APPROVE, ACTION_DECLINE -> {
                val approval = intent.getStringExtra(EXTRA_APPROVAL) ?: return
                val approve = intent.action == ACTION_APPROVE
                val serviced = core.paired() && KaiService.start(context)
                val pending = if (serviced) null else goAsync()
                app.scope.launch {
                    try {
                        when (val d = core.decide(approval, approve)) {
                            // The core raises the reply's waiting calls again, naming this verdict.
                            is Decided.Waiting -> Log.i(TAG, "a verdict from a notification is held for ${d.remaining} more call(s) of its reply")
                            Decided.Sent -> {}
                        }
                    } catch (e: CoreException) {
                        // Its words name the tool: they go to the notice, not to logcat.
                        Log.w(TAG, "a decision from a notification was not taken (${e.javaClass.simpleName})")
                        app.notifier.notice(e.message ?: "the decision was not taken")
                    } finally {
                        pending?.finish()
                    }
                }
            }
            ACTION_DISMISSED -> when (id?.kind) {
                NotificationKind.APPROVAL -> {
                    // Acknowledged: the receiver finishes once approvals.json holds the dismissal,
                    // within DISMISS_WAIT_MS (below the 10 s a broadcast receiver has), or logs why.
                    val pending = goAsync()
                    app.scope.launch {
                        try {
                            val written = withTimeoutOrNull(DISMISS_WAIT_MS) { core.dismissApproval(id.key) }
                            if (written == null) {
                                Log.w(TAG, "the dismissal of ${id.key} was not written within $DISMISS_WAIT_MS ms; the receiver finished without it")
                            }
                        } catch (e: CoreException) {
                            Log.w(TAG, "the dismissal of ${id.key} was not written: ${e.message}")
                        } finally {
                            pending.finish()
                        }
                    }
                }
                null -> {}
                else -> core.notificationGone(id)
            }
        }
    }

    companion object {
        const val ACTION_APPROVE = "dev.rootshell.kai.action.APPROVE"
        const val ACTION_DECLINE = "dev.rootshell.kai.action.DECLINE"
        const val ACTION_DISMISSED = "dev.rootshell.kai.action.DISMISSED"
        const val ACTION_CALL_MUTE = "dev.rootshell.kai.action.CALL_MUTE"
        const val ACTION_CALL_UNMUTE = "dev.rootshell.kai.action.CALL_UNMUTE"
        const val ACTION_CALL_END = "dev.rootshell.kai.action.CALL_END"
        const val ACTION_CALL_TAKE_OVER = "dev.rootshell.kai.action.CALL_TAKE_OVER"
        const val EXTRA_TAG = "tag"
        const val EXTRA_APPROVAL = "approval"
        private const val TAG = "kai"

        /** How long a dismissal's receiver waits for approvals.json (a local write: milliseconds). */
        private const val DISMISS_WAIT_MS = 5_000L
    }
}

/**
 * The one alarm (design §12.2): an inexact `setAndAllowWhileIdle` on `ELAPSED_REALTIME_WAKEUP` for
 * the earliest deadline the core needs the CPU for (`Update::WakeAt`); its receiver calls
 * `Core::wake`, which compares the deadlines with the boot clock. No exact-alarm permission; Doze may
 * defer it to a maintenance window.
 */
object WakeAlarm {
    fun set(context: Context, bootMs: ULong?) {
        val alarms = context.getSystemService(AlarmManager::class.java)
        val pi = PendingIntent.getBroadcast(
            context,
            0,
            Intent(context, WakeReceiver::class.java),
            PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT,
        )
        if (bootMs == null) {
            alarms.cancel(pi)
        } else {
            alarms.setAndAllowWhileIdle(AlarmManager.ELAPSED_REALTIME_WAKEUP, bootMs.toLong(), pi)
        }
    }
}

class WakeReceiver : BroadcastReceiver() {
    override fun onReceive(context: Context, intent: Intent) {
        (context.applicationContext as KaiApplication).core.wake()
    }
}

/**
 * Kai's service after a boot and after an update (design §12.1), when a pairing is stored;
 * `specialUse` may be started from `BOOT_COMPLETED` (Android 15's list does not name it).
 */
class BootReceiver : BroadcastReceiver() {
    override fun onReceive(context: Context, intent: Intent) {
        when (intent.action) {
            Intent.ACTION_BOOT_COMPLETED, Intent.ACTION_MY_PACKAGE_REPLACED -> {
                val app = context.applicationContext as KaiApplication
                if (app.core.paired()) KaiService.start(context)
            }
        }
    }
}
