package dev.rootshell.kai.assist

import android.annotation.SuppressLint
import android.app.PendingIntent
import android.content.Intent
import android.os.Build
import android.service.quicksettings.Tile
import android.service.quicksettings.TileService
import dev.rootshell.kai.TalkActivity

/**
 * The Quick Settings tile "Talk to Kai" (design §9.4): it opens the voice screen, which starts a
 * call. It works without the assistant role and where the phone's maker keeps the power button for
 * its own assistant. On the lock screen it asks for the unlock first (§9.6): nothing of Kai's
 * starts or shows over the keyguard.
 */
class TalkTileService : TileService() {
    override fun onStartListening() {
        super.onStartListening()
        qsTile?.apply {
            state = Tile.STATE_INACTIVE
            updateTile()
        }
    }

    override fun onClick() {
        super.onClick()
        if (isLocked) unlockAndRun { open() } else open()
    }

    @SuppressLint("StartActivityAndCollapseDeprecated")
    private fun open() {
        val intent = TalkActivity.intent(this).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)
        if (Build.VERSION.SDK_INT >= 34) {
            // The Intent overload throws for apps targeting 34 and later (design §9.4).
            startActivityAndCollapse(PendingIntent.getActivity(this, 0, intent, PendingIntent.FLAG_IMMUTABLE))
        } else {
            @Suppress("DEPRECATION")
            startActivityAndCollapse(intent)
        }
    }
}
