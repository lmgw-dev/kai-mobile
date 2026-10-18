package dev.rootshell.kai.app

import dev.rootshell.kai.core.ffi.CaptionLine
import dev.rootshell.kai.core.ffi.MessageView
import dev.rootshell.kai.core.ffi.MessagesDelta
import dev.rootshell.kai.core.ffi.NotificationId
import dev.rootshell.kai.core.ffi.NotificationKind
import java.security.MessageDigest

/**
 * The Kai thread's messages as the screen shows them, merged from the core's deltas by message id
 * (design §3.1: a collector that falls behind merges and loses nothing). lmgw's ids ascend in the
 * thread's order; the reply streaming to the phone's own send (a negative id) comes last.
 */
object MessageList {
    fun apply(current: List<MessageView>, delta: MessagesDelta): List<MessageView> {
        val byId = LinkedHashMap<Long, MessageView>()
        if (!delta.reset) current.forEach { byId[it.id] = it }
        delta.removed.forEach { byId.remove(it) }
        delta.upsert.forEach { byId[it.id] = it }
        return byId.values.sortedWith(compareBy<MessageView>({ it.id < 0 }, { if (it.id < 0) -it.id else it.id }))
    }
}

/**
 * A call's captions as the voice screen shows them (design §7.6), merged by line id: a line of a
 * newer call starts the list again, a line of an older one is dropped.
 */
object CaptionList {
    fun apply(current: List<CaptionLine>, line: CaptionLine): List<CaptionLine> {
        val call = current.firstOrNull()?.call
        return when {
            call == null || line.call > call -> listOf(line)
            line.call < call -> current
            else -> (current.filter { it.id != line.id } + line).sortedBy { it.id }
        }
    }
}

/**
 * A notification's tag (design §12.3: stable ids): `approval:<id>`, `reply:<thread>`,
 * `notice:<key>`, so the system's active notifications say after a restart what the core
 * notified for.
 */
object NotificationTags {
    /**
     * A notice's key: the first 16 hex digits of its text's SHA-256, so each sentence has a
     * notification of its own (a second notice never replaces the first silently) and the same
     * sentence again replaces itself.
     */
    fun noticeKey(text: String): String =
        MessageDigest.getInstance("SHA-256").digest(text.toByteArray(Charsets.UTF_8))
            .take(8).joinToString("") { "%02x".format(it) }

    fun tag(id: NotificationId): String = when (id.kind) {
        NotificationKind.APPROVAL -> "$APPROVAL${id.key}"
        NotificationKind.REPLY -> "$REPLY${id.key}"
        NotificationKind.NOTICE -> "$NOTICE${id.key}"
    }

    fun parse(tag: String?): NotificationId? = when {
        tag == null -> null
        tag.startsWith(APPROVAL) -> NotificationId(NotificationKind.APPROVAL, tag.removePrefix(APPROVAL))
        tag.startsWith(REPLY) -> NotificationId(NotificationKind.REPLY, tag.removePrefix(REPLY))
        tag.startsWith(NOTICE) -> NotificationId(NotificationKind.NOTICE, tag.removePrefix(NOTICE))
        else -> null
    }

    /** The approval ids and reply threads among `tags` (the active notifications at start). */
    fun active(tags: List<String?>): Pair<List<String>, List<Long>> {
        val ids = tags.mapNotNull { parse(it) }
        return ids.filter { it.kind == NotificationKind.APPROVAL }.map { it.key } to
            ids.filter { it.kind == NotificationKind.REPLY }.mapNotNull { it.key.toLongOrNull() }
    }

    private const val APPROVAL = "approval:"
    private const val REPLY = "reply:"
    private const val NOTICE = "notice:"
}

/** A pairing link handed over by a tapped `lmgw-pair:` URI or a scan; the core's parser decides. */
object PairingLinks {
    fun of(text: String?): String? = text?.trim()?.takeIf { it.startsWith("lmgw-pair:", ignoreCase = true) }
}
