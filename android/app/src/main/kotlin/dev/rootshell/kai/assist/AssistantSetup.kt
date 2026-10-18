package dev.rootshell.kai.assist

import android.Manifest
import android.app.role.RoleManager
import android.content.Context
import android.content.pm.PackageManager
import android.os.Build
import android.provider.Settings
import androidx.core.content.ContextCompat
import androidx.core.content.edit

/**
 * Setup's "Kai as your digital assistant" card (design §9.5): the role, the button that invokes it
 * on this phone, and the microphone, re-checked when the page resumes. The pure part decides the
 * open steps; [facts] reads them from Android.
 */
data class AssistantFacts(
    /** The system offers the assistant role at all. */
    val roleAvailable: Boolean,
    val roleHeld: Boolean,
    /** `RECORD_AUDIO` is granted (the overlay cannot ask for it, §9.5 step 4). */
    val microphone: Boolean,
    val maker: Maker,
    /** `Settings.Global.power_button_long_press` as read (null: not set, or not readable). */
    val powerLongPress: Int?,
    /** Reading it was refused (a `SecurityException`): the value is unknown, not the build's default. */
    val powerUnreadable: Boolean = false,
    /** The owner ticked the button's step done (One UI and most phones offer no way to read it). */
    val buttonTicked: Boolean,
)

enum class AssistantStep { Role, Button, Microphone }

/**
 * The phone's maker, for the button's steps only (§9.4, §9.5 step 3), from what the build says:
 * LineageOS by its product or display name (also on a phone a maker sold), then the manufacturer.
 */
enum class Maker {
    SAMSUNG,
    LINEAGE,
    PIXEL,
    OTHER,
    ;

    companion object {
        fun of(manufacturer: String, product: String, display: String): Maker = when {
            product.startsWith("lineage", ignoreCase = true) || display.contains("lineage", ignoreCase = true) -> LINEAGE
            manufacturer.equals("samsung", ignoreCase = true) -> SAMSUNG
            manufacturer.equals("google", ignoreCase = true) -> PIXEL
            else -> OTHER
        }
    }
}

/** How "Set Kai" asks for the assistant role (§9.5 step 2). */
enum class SetKai {
    /** `Settings.ACTION_VOICE_INPUT_SETTINGS` (the digital assistant's setting), the role re-checked on resume. */
    VOICE_INPUT_SETTINGS,

    /** `RoleManager.createRequestRoleIntent(ROLE_ASSISTANT)`, a dialog where the system shows one. */
    ROLE_REQUEST,
}

object AssistantSetup {
    /**
     * **WP0 (a)'s seam for setup:** how "Set Kai" asks for the role. The design's default (§9.5):
     * the deep link, because `createRequestRoleIntent(ROLE_ASSISTANT)` reportedly shows no dialog;
     * if WP0 (a) finds that it does on One UI and LineageOS, it replaces the deep link here.
     */
    val SET_KAI: SetKai = SetKai.VOICE_INPUT_SETTINGS

    /**
     * `power_button_long_press`'s value for the assistant (AOSP's `LONG_PRESS_POWER_ASSISTANT`), the
     * one LineageOS needs (§9.4; WP0 (a) confirms whether its Settings offer it).
     */
    const val POWER_ASSISTANT = 5

    /** The steps still open, in the card's order. */
    fun open(f: AssistantFacts): List<AssistantStep> = buildList {
        if (!f.roleHeld) add(AssistantStep.Role)
        if (!buttonDone(f)) add(AssistantStep.Button)
        if (!f.microphone) add(AssistantStep.Microphone)
    }

    /** The button's step: ticked by the owner, or read where the system says it (LineageOS's power button). */
    fun buttonDone(f: AssistantFacts): Boolean =
        f.buttonTicked || (f.maker == Maker.LINEAGE && f.powerLongPress == POWER_ASSISTANT)

    fun facts(context: Context): AssistantFacts {
        val roles = context.getSystemService(RoleManager::class.java)
        val power = runCatching {
            Settings.Global.getString(context.contentResolver, POWER_LONG_PRESS)?.trim()?.toIntOrNull()
        }
        return AssistantFacts(
            roleAvailable = roles?.isRoleAvailable(RoleManager.ROLE_ASSISTANT) == true,
            roleHeld = roles?.isRoleHeld(RoleManager.ROLE_ASSISTANT) == true,
            microphone = ContextCompat.checkSelfPermission(context, Manifest.permission.RECORD_AUDIO) == PackageManager.PERMISSION_GRANTED,
            maker = Maker.of(Build.MANUFACTURER, Build.PRODUCT, Build.DISPLAY),
            powerLongPress = power.getOrNull(),
            powerUnreadable = power.exceptionOrNull() is SecurityException,
            buttonTicked = prefs(context).getBoolean(BUTTON_TICKED, false),
        )
    }

    fun tickButton(context: Context, done: Boolean) {
        prefs(context).edit { putBoolean(BUTTON_TICKED, done) }
    }

    private fun prefs(context: Context) = context.getSharedPreferences("setup", Context.MODE_PRIVATE)

    /** `Settings.Global.POWER_BUTTON_LONG_PRESS`, hidden from the SDK. */
    private const val POWER_LONG_PRESS = "power_button_long_press"
    private const val BUTTON_TICKED = "assistant_button_ticked"
}
