package dev.rootshell.kai.app

import android.app.NotificationManager
import android.content.Context
import android.os.Build
import android.os.PowerManager
import androidx.core.content.edit

/**
 * Setup's "Kai in the background" card (design §9.5): what makes the held feed survive. Each step is
 * re-checked when the screen resumes; Samsung's "Never sleeping apps" list has no intent and no way
 * to read it, so the owner ticks that step done (kept in the app's preferences).
 */
data class BackgroundFacts(
    val notificationsAllowed: Boolean,
    val batteryExempt: Boolean,
    val samsung: Boolean,
    val neverSleepingTicked: Boolean,
)

enum class BackgroundStep { Notifications, Battery, NeverSleeping }

object BackgroundSetup {
    /** The steps still open, in the card's order. */
    fun open(f: BackgroundFacts): List<BackgroundStep> = buildList {
        if (!f.notificationsAllowed) add(BackgroundStep.Notifications)
        if (!f.batteryExempt) add(BackgroundStep.Battery)
        if (f.samsung && !f.neverSleepingTicked) add(BackgroundStep.NeverSleeping)
    }

    fun facts(context: Context): BackgroundFacts = BackgroundFacts(
        // POST_NOTIFICATIONS from Android 13; the switch covers both that and an older phone's.
        notificationsAllowed = context.getSystemService(NotificationManager::class.java).areNotificationsEnabled(),
        batteryExempt = context.getSystemService(PowerManager::class.java).isIgnoringBatteryOptimizations(context.packageName),
        samsung = Build.MANUFACTURER.equals("samsung", ignoreCase = true),
        neverSleepingTicked = prefs(context).getBoolean(NEVER_SLEEPING, false),
    )

    fun tickNeverSleeping(context: Context, done: Boolean) {
        prefs(context).edit { putBoolean(NEVER_SLEEPING, done) }
    }

    private fun prefs(context: Context) = context.getSharedPreferences("setup", Context.MODE_PRIVATE)

    private const val NEVER_SLEEPING = "never_sleeping_ticked"
}
