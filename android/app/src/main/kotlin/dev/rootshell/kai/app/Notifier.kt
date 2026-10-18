package dev.rootshell.kai.app

import android.app.KeyguardManager
import android.app.Notification
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.content.BroadcastReceiver
import android.content.Context
import android.content.Intent
import android.content.IntentFilter
import android.graphics.drawable.Icon
import android.os.PowerManager
import androidx.core.content.ContextCompat
import dev.rootshell.kai.NotificationTapActivity
import dev.rootshell.kai.R
import dev.rootshell.kai.app.LockScreen.Face
import dev.rootshell.kai.app.LockScreen.Kind
import dev.rootshell.kai.core.ffi.CallEnd
import dev.rootshell.kai.core.ffi.CallPhase
import dev.rootshell.kai.core.ffi.ConnectionState
import dev.rootshell.kai.core.ffi.ConnectionView
import dev.rootshell.kai.core.ffi.NotificationId
import dev.rootshell.kai.core.ffi.NotificationKind
import dev.rootshell.kai.core.ffi.NotificationView
import dev.rootshell.kai.core.ffi.VoiceAction
import dev.rootshell.kai.core.ffi.VoiceView
import java.text.DateFormat
import java.util.Date

/**
 * The notifications (design §12.3), built from what the core hands over, never from a fetch:
 * channels `status` (low, silent: the service's one notification), `approvals` (high), `messages`
 * (Kai's replies) and `notices`. Every notification is tagged by its stable id
 * ([NotificationTags]) and alerts once. An approval's Approve needs an unlocked phone
 * (`setAuthenticationRequired`), Decline does not; both go to [NotificationActionReceiver], as does
 * a dismissal. A tap and Talk go through [NotificationTapActivity], which is not exported; no
 * action starts voice (design M16).
 *
 * Privacy by default on the lock screen (§12.3, [LockScreen]): while the phone is locked (or its
 * screen off) each notification is posted with the content-free face of its kind as its main
 * content, whatever Android's lock-screen setting says, and posted again with its content when the
 * owner is present ([watchTheLock], [refresh]). `VISIBILITY_PRIVATE` with a content-free public
 * version stays as the second line. Each intent has an identity of its own (`Intent.setIdentifier`:
 * the tag, with the action), so no two notifications' PendingIntents can be taken for one another
 * and rewrite each other's extras.
 */
class Notifier(private val context: Context) {
    private val manager = context.getSystemService(NotificationManager::class.java)
    private val keyguard = context.getSystemService(KeyguardManager::class.java)
    private val power = context.getSystemService(PowerManager::class.java)

    /**
     * A notification Kai shows: how it is built with either face, the face it was last posted
     * with, and whether a refresh found it in Android's list (then, missing from it, it is gone).
     */
    private class Posted(val build: (Boolean) -> Notification, var locked: Boolean) {
        var seen = false
    }

    /** The notifications Kai shows, by tag and id (the service's: no tag); guarded by itself. */
    private val posted = HashMap<Pair<String?, Int>, Posted>()

    fun createChannels() {
        manager.createNotificationChannels(
            listOf(
                NotificationChannel(CHANNEL_STATUS, context.getString(R.string.channel_status), NotificationManager.IMPORTANCE_LOW).apply {
                    setSound(null, null)
                    setShowBadge(false)
                },
                NotificationChannel(CHANNEL_APPROVALS, context.getString(R.string.channel_approvals), NotificationManager.IMPORTANCE_HIGH),
                NotificationChannel(CHANNEL_MESSAGES, context.getString(R.string.channel_messages), NotificationManager.IMPORTANCE_DEFAULT),
                NotificationChannel(CHANNEL_NOTICES, context.getString(R.string.channel_notices), NotificationManager.IMPORTANCE_DEFAULT),
            ),
        )
    }

    /** The tags of the notifications shown now (they outlive the process). */
    fun activeTags(): List<String?> = runCatching { manager.activeNotifications.map { it.tag } }.getOrDefault(emptyList())

    /**
     * Follows the lock for the process's life (design §12.3): the screen going off or on and the
     * owner present after unlocking post every notification again with the face it needs then.
     * These broadcasts reach only a receiver registered at run time; Kai's service keeps the process.
     */
    fun watchTheLock() {
        val receiver = object : BroadcastReceiver() {
            override fun onReceive(context: Context, intent: Intent) = refresh()
        }
        val filter = IntentFilter().apply {
            addAction(Intent.ACTION_SCREEN_OFF)
            addAction(Intent.ACTION_SCREEN_ON)
            addAction(Intent.ACTION_USER_PRESENT)
        }
        ContextCompat.registerReceiver(context, receiver, filter, ContextCompat.RECEIVER_NOT_EXPORTED)
    }

    /** Whether what is posted now may be seen on the lock screen ([LockScreen.locked]). */
    fun locked(): Boolean = LockScreen.locked(keyguard?.isKeyguardLocked == true, power?.isInteractive != false)

    /**
     * The lock changed: every notification still shown is posted again with the face it needs now.
     * One a refresh found in Android's list and that is no longer there is gone (the owner cleared
     * it); one not listed yet may still be on its way into the list, so it is posted again rather
     * than left with the wrong face.
     */
    fun refresh() {
        synchronized(posted) {
            val locked = locked()
            val active = runCatching { manager.activeNotifications.map { it.tag to it.id }.toSet() }.getOrNull()
            val entries = posted.entries.iterator()
            while (entries.hasNext()) {
                val (key, p) = entries.next()
                val shown = active?.contains(key) ?: true
                if (!shown && p.seen) {
                    entries.remove()
                    continue
                }
                if (shown) p.seen = true
                if (p.locked == locked) continue
                p.locked = locked
                runCatching { manager.notify(key.first, key.second, p.build(locked)) }
            }
        }
    }

    /** A notification went (withdrawn, cleared, tapped away): it is never posted again. */
    fun gone(tag: String?, id: Int = ID) {
        synchronized(posted) { posted.remove(tag to id) }
    }

    /**
     * Builds the notification with the face of now and keeps how, for the lock's changes; posts it
     * unless `post` is false (the service's, which `startForeground` posts).
     */
    private fun keep(tag: String?, id: Int, post: Boolean, build: (Boolean) -> Notification): Notification =
        synchronized(posted) {
            val locked = locked()
            val n = build(locked)
            posted[tag to id] = Posted(build, locked)
            if (post) runCatching { manager.notify(tag, id, n) }
            n
        }

    private fun words(id: Int): String = context.getString(id)

    fun show(n: NotificationView) {
        val tag = NotificationTags.tag(n.id)
        val kind = kind(n.id.kind)
        val count = if (n.id.kind == NotificationKind.REPLY && n.count > 1u) {
            context.resources.getQuantityString(R.plurals.replies_count, n.count.toInt(), n.count.toInt())
        } else {
            null
        }
        val full = Face(n.title, n.text, bigText = n.text, subText = count)
        val at = System.currentTimeMillis()
        keep(tag, ID, post = true) { locked -> build(n, tag, LockScreen.face(kind, full, locked, ::words), at) }
    }

    /** An approval, a reply or a notice with `face` (its actions only where the face keeps them). */
    private fun build(n: NotificationView, tag: String, face: Face, at: Long): Notification {
        val builder = Notification.Builder(context, channel(n.id.kind))
            .setSmallIcon(R.drawable.ic_notification)
            .setContentTitle(face.title)
            .setContentText(face.text)
            .setWhen(at)
            .setOnlyAlertOnce(true)
            .setVisibility(Notification.VISIBILITY_PRIVATE)
            .setPublicVersion(public(kind(n.id.kind)))
            .setContentIntent(open(tag))
            .setDeleteIntent(action(NotificationActionReceiver.ACTION_DISMISSED, tag, n))
        face.bigText?.let { builder.setStyle(Notification.BigTextStyle().bigText(it)) }
        face.subText?.let { builder.setSubText(it) }
        when (n.id.kind) {
            NotificationKind.APPROVAL -> {
                builder.setCategory(Notification.CATEGORY_REMINDER).setAutoCancel(false)
                if (face.actions) {
                    builder.addAction(
                        Notification.Action.Builder(
                            Icon.createWithResource(context, R.drawable.ic_notification),
                            context.getString(R.string.approve),
                            action(NotificationActionReceiver.ACTION_APPROVE, tag, n),
                        ).setAuthenticationRequired(true).build(),
                    )
                        .addAction(
                            Notification.Action.Builder(
                                Icon.createWithResource(context, R.drawable.ic_notification),
                                context.getString(R.string.decline),
                                action(NotificationActionReceiver.ACTION_DECLINE, tag, n),
                            ).build(),
                        )
                }
            }
            NotificationKind.REPLY -> {
                builder.setCategory(Notification.CATEGORY_MESSAGE).setAutoCancel(true)
                if (face.actions) {
                    builder.addAction(
                        Notification.Action.Builder(
                            Icon.createWithResource(context, R.drawable.ic_notification),
                            context.getString(R.string.talk),
                            open(tag, talk = true),
                        ).build(),
                    )
                }
            }
            NotificationKind.NOTICE -> builder.setAutoCancel(true)
        }
        return builder.build()
    }

    fun withdraw(id: NotificationId) {
        val tag = NotificationTags.tag(id)
        gone(tag)
        manager.cancel(tag, ID)
    }

    /**
     * A one-time sentence as a notice (when the app is not on screen), tagged by its text
     * ([NotificationTags.noticeKey]): another notice never replaces it, the same sentence again does.
     */
    fun notice(text: String) {
        show(NotificationView(NotificationId(NotificationKind.NOTICE, NotificationTags.noticeKey(text)), context.getString(R.string.app_name), text, null, null, 1u))
    }

    /** The service's one notification (design §12.1): the link's state, updated on a change. */
    fun status(view: ConnectionView?): Notification {
        val text = when {
            view == null -> context.getString(R.string.status_starting)
            view.state == ConnectionState.CONNECTED -> context.getString(R.string.status_connected)
            view.nextTryAtMs != null -> context.getString(
                R.string.status_next_try,
                view.words,
                DateFormat.getTimeInstance(DateFormat.SHORT).format(Date(view.nextTryAtMs!!)),
            )
            else -> view.words
        }
        val full = Face(context.getString(R.string.app_name), text)
        return keep(null, STATUS_ID, post = false) { locked ->
            val face = LockScreen.face(Kind.STATUS, full, locked, ::words)
            Notification.Builder(context, CHANNEL_STATUS)
                .setSmallIcon(R.drawable.ic_notification)
                .setContentTitle(face.title)
                .setContentText(face.text)
                .setOngoing(true)
                .setOnlyAlertOnce(true)
                .setVisibility(Notification.VISIBILITY_PRIVATE)
                .setPublicVersion(public(Kind.STATUS))
                .setContentIntent(open(null))
                .build()
        }
    }

    fun showStatus(view: ConnectionView?) {
        runCatching { manager.notify(STATUS_ID, status(view)) }
    }

    /**
     * The service's notification during a call (design §8.6): the state and the thread ("listening ·
     * Kai: <title> · ends after 30 s of quiet"), Mute or Unmute and End (Take over while a bind is
     * pending), the tap opening the voice screen; a call's category, so it shows on the lock screen
     * like a call's. Privacy by default there (§12.3): while locked it says "Kai · voice call" with
     * the same actions and no thread or words. The idle end is left out while a call of the thread
     * waits for the owner's verdict: the idle clock stands still then (§7.7).
     */
    fun call(view: VoiceView?, idleEndS: UInt): Notification {
        val parts = mutableListOf(view?.words?.takeIf { it.isNotBlank() } ?: context.getString(R.string.call_starting))
        view?.threadTitle?.let { parts += context.getString(R.string.call_thread, it) }
        val quietEnds = (view?.phase == CallPhase.LIVE || view?.phase == CallPhase.PENDING) && view.awaiting.isEmpty()
        if (quietEnds && idleEndS > 0u) parts += context.getString(R.string.call_idle_end, idleEndS.toInt())
        val actions = callActions(view)
        val full = Face(context.getString(R.string.app_name), parts.joinToString(" · "))
        return keep(null, STATUS_ID, post = false) { locked ->
            val face = LockScreen.face(Kind.CALL, full, locked, ::words)
            val builder = Notification.Builder(context, CHANNEL_STATUS)
                .setSmallIcon(R.drawable.ic_notification)
                .setContentTitle(face.title)
                .setContentText(face.text)
                .setOngoing(true)
                .setOnlyAlertOnce(true)
                .setCategory(Notification.CATEGORY_CALL)
                .setVisibility(Notification.VISIBILITY_PRIVATE)
                .setPublicVersion(public(Kind.CALL, actions))
                .setContentIntent(open(null))
            if (face.actions) actions.forEach { builder.addAction(it) }
            builder.build()
        }
    }

    fun showCall(view: VoiceView?, idleEndS: UInt) {
        runCatching { manager.notify(STATUS_ID, call(view, idleEndS)) }
    }

    private fun callActions(view: VoiceView?): List<Notification.Action> {
        fun action(label: Int, what: String) = Notification.Action.Builder(
            Icon.createWithResource(context, R.drawable.ic_notification),
            context.getString(label),
            PendingIntent.getBroadcast(
                context,
                0,
                Intent(context, NotificationActionReceiver::class.java).setAction(what).setIdentifier(what),
                PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT,
            ),
        ).build()
        return buildList {
            when {
                view?.phase == CallPhase.PENDING -> add(action(R.string.take_over, NotificationActionReceiver.ACTION_CALL_TAKE_OVER))
                view?.muted == true -> add(action(R.string.unmute, NotificationActionReceiver.ACTION_CALL_UNMUTE))
                else -> add(action(R.string.mute, NotificationActionReceiver.ACTION_CALL_MUTE))
            }
            add(action(R.string.end_call, NotificationActionReceiver.ACTION_CALL_END))
        }
    }

    /**
     * A call that ended while the app was not on screen (design §7.9, §12.3: voice moved or stayed
     * elsewhere, connection lost): its words on channel `notices`, with Take over where the end
     * offers it, which opens the voice screen to start the call (a push never starts voice).
     */
    fun voiceNotice(end: CallEnd) {
        val said = end.words ?: return
        val text = listOfNotNull(said, end.message).joinToString("\n")
        val tag = NotificationTags.tag(NotificationId(NotificationKind.NOTICE, VOICE_NOTICE))
        val full = Face(context.getString(R.string.app_name), said, bigText = text)
        val at = System.currentTimeMillis()
        keep(tag, ID, post = true) { locked ->
            val face = LockScreen.face(Kind.NOTICE, full, locked, ::words)
            val builder = Notification.Builder(context, CHANNEL_NOTICES)
                .setSmallIcon(R.drawable.ic_notification)
                .setContentTitle(face.title)
                .setContentText(face.text)
                .setWhen(at)
                .setAutoCancel(true)
                .setVisibility(Notification.VISIBILITY_PRIVATE)
                .setPublicVersion(public(Kind.NOTICE))
                .setContentIntent(open(tag))
                .setDeleteIntent(dismissed(tag))
            face.bigText?.let { builder.setStyle(Notification.BigTextStyle().bigText(it)) }
            if (face.actions && VoiceAction.TAKE_OVER in end.actions) {
                builder.addAction(
                    Notification.Action.Builder(
                        Icon.createWithResource(context, R.drawable.ic_notification),
                        context.getString(R.string.take_over),
                        open(tag, takeOver = true),
                    ).build(),
                )
            }
            builder.build()
        }
    }

    private fun channel(kind: NotificationKind) = when (kind) {
        NotificationKind.APPROVAL -> CHANNEL_APPROVALS
        NotificationKind.REPLY -> CHANNEL_MESSAGES
        NotificationKind.NOTICE -> CHANNEL_NOTICES
    }

    private fun kind(kind: NotificationKind) = when (kind) {
        NotificationKind.APPROVAL -> Kind.APPROVAL
        NotificationKind.REPLY -> Kind.REPLY
        NotificationKind.NOTICE -> Kind.NOTICE
    }

    /**
     * The public version of a notification of `kind` (the second line of the lock screen's privacy):
     * its content-free text, and a call's actions.
     */
    private fun public(kind: Kind, actions: List<Notification.Action> = emptyList()): Notification {
        val channel = when (kind) {
            Kind.APPROVAL -> CHANNEL_APPROVALS
            Kind.REPLY -> CHANNEL_MESSAGES
            Kind.NOTICE -> CHANNEL_NOTICES
            Kind.STATUS, Kind.CALL -> CHANNEL_STATUS
        }
        val builder = Notification.Builder(context, channel)
            .setSmallIcon(R.drawable.ic_notification)
            .setContentTitle(context.getString(R.string.app_name))
            .setContentText(context.getString(LockScreen.contentFree(kind)))
        if (kind == Kind.CALL) builder.setCategory(Notification.CATEGORY_CALL)
        if (LockScreen.actionsWhileLocked(kind)) actions.forEach { builder.addAction(it) }
        return builder.build()
    }

    /** A notice cleared by the owner: it is never posted again. */
    private fun dismissed(tag: String): PendingIntent {
        val intent = Intent(context, NotificationActionReceiver::class.java)
            .setAction(NotificationActionReceiver.ACTION_DISMISSED)
            .setIdentifier(tag)
            .putExtra(NotificationActionReceiver.EXTRA_TAG, tag)
        return PendingIntent.getBroadcast(context, 0, intent, PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT)
    }

    /** A tap (or Talk, or Take over) through the trampoline; `tag` null: the status notification. */
    private fun open(tag: String?, talk: Boolean = false, takeOver: Boolean = false): PendingIntent {
        val action = when {
            takeOver -> NotificationTapActivity.ACTION_TAKE_OVER
            talk -> NotificationTapActivity.ACTION_TALK
            else -> NotificationTapActivity.ACTION_OPEN
        }
        val intent = Intent(context, NotificationTapActivity::class.java)
            .setAction(action)
            .setIdentifier("${tag ?: STATUS_IDENTITY}:$action")
            .setFlags(Intent.FLAG_ACTIVITY_NEW_TASK)
            .putExtra(NotificationTapActivity.EXTRA_TAG, tag)
        return PendingIntent.getActivity(context, 0, intent, PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT)
    }

    private fun action(action: String, tag: String, n: NotificationView): PendingIntent {
        val intent = Intent(context, NotificationActionReceiver::class.java)
            .setAction(action)
            .setIdentifier(tag)
            .putExtra(NotificationActionReceiver.EXTRA_TAG, tag)
        n.approval?.let {
            intent.putExtra(NotificationActionReceiver.EXTRA_APPROVAL, it.approvalRequestId)
        }
        return PendingIntent.getBroadcast(context, 0, intent, PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT)
    }

    companion object {
        const val CHANNEL_STATUS = "status"
        const val CHANNEL_APPROVALS = "approvals"
        const val CHANNEL_MESSAGES = "messages"
        const val CHANNEL_NOTICES = "notices"
        const val ID = 1
        const val STATUS_ID = 100

        /** The status notification's intent identity (the others carry their tag). */
        private const val STATUS_IDENTITY = "status"

        /** The voice notice's key: one at a time, the latest end replacing the last. */
        const val VOICE_NOTICE = "voice"
    }
}
