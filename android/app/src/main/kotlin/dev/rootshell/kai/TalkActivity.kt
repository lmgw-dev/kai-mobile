package dev.rootshell.kai

import android.app.Activity
import android.app.KeyguardManager
import android.content.Context
import android.content.Intent
import android.os.Bundle
import android.util.Log
import dev.rootshell.kai.core.ffi.CallStart
import dev.rootshell.kai.ui.Routes

/**
 * "Talk to Kai" from outside the app's screens (design §9.4): the Quick Settings tile, the launcher
 * shortcut, and the assistant overlay's Unlock and Open Kai. It draws nothing and hands over to
 * [MainActivity]'s voice screen, which starts the call as a visible activity may (§7.3, M16), asking
 * for the microphone first where needed. Never exported: only Kai's own tile, shortcut and overlay
 * reach it, so no other app can make Kai listen through it.
 *
 * **The lock screen** (§9.6, privacy by default): it shows over the keyguard only to ask for the
 * unlock (`requestDismissKeyguard`), and nothing starts or shows until the phone is unlocked; a
 * cancelled unlock ends it.
 */
class TalkActivity : Activity() {
    private var asked = false

    override fun onResume() {
        super.onResume()
        if (asked) return
        asked = true
        val keyguard = getSystemService(KeyguardManager::class.java)
        if (keyguard?.isKeyguardLocked != true) {
            talk()
            return
        }
        keyguard.requestDismissKeyguard(
            this,
            object : KeyguardManager.KeyguardDismissCallback() {
                override fun onDismissSucceeded() = talk()

                override fun onDismissCancelled() {
                    Log.i(TAG, "talk: the unlock was cancelled; no call")
                    finish()
                }

                override fun onDismissError() {
                    Log.w(TAG, "talk: the unlock could not be asked for; no call")
                    finish()
                }
            },
        )
    }

    private fun talk() {
        val app = application as KaiApplication
        app.hub.pendingRoute.value = Routes.MAIN
        // The voice screen starts it unless a call runs already (it shows that one).
        app.hub.pendingCall.value = CallStart(new = false, takeOver = false)
        // Into Kai's own task: this one has none (no affinity).
        startActivity(
            Intent(this, MainActivity::class.java)
                .addFlags(Intent.FLAG_ACTIVITY_NEW_TASK or Intent.FLAG_ACTIVITY_CLEAR_TOP or Intent.FLAG_ACTIVITY_SINGLE_TOP),
        )
        finish()
    }

    companion object {
        private const val TAG = "kai"

        /** Its intent, for the tile and the overlay. */
        fun intent(context: Context): Intent =
            Intent(context, TalkActivity::class.java).setAction(ACTION_TALK)

        /** The action the tile, the shortcut and the overlay start it with. */
        const val ACTION_TALK = "dev.rootshell.kai.action.TALK_TO_KAI"
    }
}
