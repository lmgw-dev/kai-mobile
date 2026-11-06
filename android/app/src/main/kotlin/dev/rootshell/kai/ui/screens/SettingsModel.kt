package dev.rootshell.kai.ui.screens

import dev.rootshell.kai.R

/**
 * What the settings pages decide without Android (design §3.4, 2026-10-19): which conversation
 * choice a stored folder name is, the name a choice saves, which text is a whole number, and the
 * human label of every setting the core can refuse by its key. The keys are the file's format and
 * never the owner's words.
 */
enum class ConversationChoice { SHARED, THIS_PHONE }

object ConversationName {
    /** The folder every Kai client uses by default, the desktop's too. */
    const val SHARED = "Kai"

    /** What the name field starts with when the device has no name to put after "Kai". */
    const val THIS_PHONE_DEFAULT = "Kai mobile"

    /** What the name field starts with for "Only on this phone": "Kai" and this device's name, so two phones differ. */
    fun thisPhoneDefault(deviceName: String): String =
        deviceName.trim().let { if (it.isEmpty()) THIS_PHONE_DEFAULT else "$SHARED $it" }

    /** The choice a stored folder name is: the shared name exactly, else a conversation of its own. */
    fun choiceOf(stored: String): ConversationChoice =
        if (stored == SHARED) ConversationChoice.SHARED else ConversationChoice.THIS_PHONE

    /** What the name field shows when the page opens: the stored own name, else the default. */
    fun fieldStart(stored: String, deviceName: String): String =
        if (stored == SHARED) thisPhoneDefault(deviceName) else stored

    /** Why a choice cannot be saved with this name field, or null when it can. */
    fun refusal(choice: ConversationChoice, typed: String): NameRefusal? = when {
        choice == ConversationChoice.SHARED -> null
        typed.isBlank() -> NameRefusal.EMPTY
        typed.trim() == SHARED -> NameRefusal.IS_SHARED
        else -> null
    }

    /** The folder name a choice saves; null where [refusal] says why not. */
    fun folderFor(choice: ConversationChoice, typed: String): String? = when {
        refusal(choice, typed) != null -> null
        choice == ConversationChoice.SHARED -> SHARED
        else -> typed.trim()
    }
}

enum class NameRefusal { EMPTY, IS_SHARED }

/** Whole numbers as the number fields take them (an optional field may be empty: not set). */
object WholeNumber {
    fun valid(text: String, optional: Boolean = false): Boolean =
        text.isBlank().let { if (it) optional else text.trim().toUIntOrNull() != null }

    fun parse(text: String): UInt = text.trim().toUInt()

    fun parseOptional(text: String): UInt? = text.trim().takeIf { it.isNotEmpty() }?.toUInt()
}

/** The settings the core can name in a refusal, each with the label the pages show (never the key). */
enum class SettingKey(val key: String, val label: Int) {
    TIMEOUT("lmgw.timeout_s", R.string.set_timeout),
    STAGGER("lmgw.url_stagger_ms", R.string.set_stagger),
    RETRY("lmgw.retry_max_s", R.string.set_retry),
    RETRY_BACKGROUND("lmgw.retry_max_background_s", R.string.set_retry_background),
    KEEPALIVE("feed.keepalive_s", R.string.set_keepalive),
    CONFIRM_POLL("pairing.confirm_poll_s", R.string.set_confirm_poll),
    FOLDER("conversation.folder_name", R.string.set_conversation_name),
    CONNECT_TIMEOUT("voice.connect_timeout_s", R.string.set_connect),
    IDLE_END("voice.idle_end_s", R.string.set_idle),
    PLAYOUT_REPORT("voice.playout_report_ms", R.string.set_report),
    PTT_TAIL("voice.ptt_tail_ms", R.string.set_ptt_tail),
    ECHO_TAIL("voice.echo_tail_ms", R.string.set_echo_tail),
    ;

    companion object {
        fun of(key: String): SettingKey? = entries.firstOrNull { it.key == key }
    }
}

/** How a refusal reads: which string, and the labels it names. */
data class RefusalWords(val text: Int, val labels: List<Int>)

object Refusals {
    /** The words for the core's refusal of the setting [key]; labels, never keys. */
    fun of(key: String): RefusalWords {
        val setting = SettingKey.of(key) ?: return RefusalWords(R.string.set_refused_unknown, emptyList())
        return when (setting) {
            SettingKey.RETRY_BACKGROUND ->
                RefusalWords(R.string.set_refused_below, listOf(setting.label, SettingKey.RETRY.label))
            SettingKey.FOLDER -> RefusalWords(R.string.set_refused_empty, listOf(setting.label))
            else -> RefusalWords(R.string.set_refused_at_least_one, listOf(setting.label))
        }
    }
}
