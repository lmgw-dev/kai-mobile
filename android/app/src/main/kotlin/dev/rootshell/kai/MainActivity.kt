package dev.rootshell.kai

import android.content.Intent
import android.os.Bundle
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.activity.enableEdgeToEdge
import dev.rootshell.kai.app.Entry
import dev.rootshell.kai.ui.KaiApp
import dev.rootshell.kai.ui.Routes
import dev.rootshell.kai.ui.theme.KaiTheme

/**
 * The one activity (design §10.2). Nothing lives in it: a fold or an unfold recreates it, and every
 * screen draws from the hub's state. It is exported (the launcher, the `lmgw-pair:` link, the
 * assistant's `ASSIST` and `VOICE_COMMAND` filters, §9.1), so what it reads from an intent is
 * [Entry]'s: a tapped `lmgw-pair:` link, which only ever leads to the pairing's confirmation (design
 * §4.1), and `ASSIST` or `VOICE_COMMAND`, which open the voice screen without a call (an exported
 * filter never makes Kai listen) or, with the keyboard hint, the conversation. A
 * notification's tap reaches it through [NotificationTapActivity], the tile and the shortcut through
 * [TalkActivity], neither exported, by the hub's state.
 */
class MainActivity : ComponentActivity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        enableEdgeToEdge()
        super.onCreate(savedInstanceState)
        val app = application as KaiApplication
        val appVersion = packageManager.getPackageInfo(packageName, 0).versionName.orEmpty()
        if (savedInstanceState == null) handle(intent)
        setContent {
            KaiTheme(colours = app.core.colours()) {
                KaiApp(app = app, about = AboutState.from(appVersion, app.core))
            }
        }
    }

    override fun onStop() {
        super.onStop()
        // A call asked for and not started (the activity never resumed, a screen locked meanwhile)
        // must not start hours later at an unlock.
        (application as KaiApplication).hub.pendingCall.value = null
    }

    override fun onNewIntent(intent: Intent) {
        super.onNewIntent(intent)
        handle(intent)
    }

    private fun handle(intent: Intent?) {
        val app = application as KaiApplication
        val keyboard = intent?.getBooleanExtra(Intent.EXTRA_ASSIST_INPUT_HINT_KEYBOARD, false) == true
        when (val entry = Entry.of(intent?.action, intent?.dataString, keyboard)) {
            is Entry.Pair -> {
                app.hub.pairingText.value = entry.link
                app.hub.pendingRoute.value = Routes.PAIRING
            }
            // No call: the owner's own tap on Talk starts one (see Entry).
            Entry.Voice -> app.hub.pendingRoute.value = Routes.MAIN
            Entry.Type -> {
                app.hub.pendingRoute.value = Routes.MAIN
                app.hub.openConversation.value = true
            }
            Entry.Open -> {}
        }
    }
}
