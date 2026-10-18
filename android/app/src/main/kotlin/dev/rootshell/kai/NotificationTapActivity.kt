package dev.rootshell.kai

import android.app.Activity
import android.content.Intent
import android.os.Bundle
import dev.rootshell.kai.app.NotificationTags
import dev.rootshell.kai.core.ffi.CallStart
import dev.rootshell.kai.core.ffi.NotificationKind
import dev.rootshell.kai.ui.Routes

/**
 * A notification's tap and its Talk action (design §12.3), never exported: only Kai's own
 * PendingIntents reach it, so the tag it carries is trusted here. [MainActivity] is exported (the
 * launcher, the `lmgw-pair:` link) and reads no notification extra, so no other app can make it
 * forget a notification or open a conversation.
 *
 * A tapped reply was cancelled with the tap (auto-cancel); Talk is an action, which cancels
 * nothing, so its reply is withdrawn here. Either way the core forgets it. A tapped approval or
 * reply opens the conversation (its sheet on the cover screen); Talk, a notice and the status
 * notification open the voice screen, and a voice notice's Take over asks the voice screen for the
 * call that takes the voice over. Then [MainActivity] comes forward and this one finishes, drawing nothing.
 */
class NotificationTapActivity : Activity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        val app = application as KaiApplication
        val id = NotificationTags.parse(intent.getStringExtra(EXTRA_TAG))
        val talk = intent.action == ACTION_TALK
        if (id?.kind == NotificationKind.REPLY) {
            if (talk) app.notifier.withdraw(id)
            app.core.notificationGone(id)
        }
        // A tapped reply or notice was cancelled with the tap (auto-cancel): never posted again at
        // the lock's next change.
        if (id?.kind == NotificationKind.REPLY || id?.kind == NotificationKind.NOTICE) {
            app.notifier.gone(intent.getStringExtra(EXTRA_TAG))
        }
        app.hub.pendingRoute.value = Routes.MAIN
        if (intent.action == ACTION_TAKE_OVER) {
            // A voice notice's Take over (design §7.9): the voice screen starts the call, which
            // takes the thread over as soon as lmgw asks.
            id?.let { app.notifier.withdraw(it) }
            app.hub.pendingCall.value = CallStart(new = false, takeOver = true)
        }
        val aboutTheConversation = id?.kind == NotificationKind.APPROVAL || id?.kind == NotificationKind.REPLY
        if (aboutTheConversation && !talk) app.hub.openConversation.value = true
        startActivity(Intent(this, MainActivity::class.java).addFlags(Intent.FLAG_ACTIVITY_CLEAR_TOP or Intent.FLAG_ACTIVITY_SINGLE_TOP))
        finish()
    }

    companion object {
        const val ACTION_OPEN = "dev.rootshell.kai.action.OPEN"
        const val ACTION_TALK = "dev.rootshell.kai.action.TALK"
        const val ACTION_TAKE_OVER = "dev.rootshell.kai.action.TAKE_OVER"
        const val EXTRA_TAG = "dev.rootshell.kai.extra.NOTIFICATION_TAG"
    }
}
