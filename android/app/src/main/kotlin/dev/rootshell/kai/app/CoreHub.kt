package dev.rootshell.kai.app

import dev.rootshell.kai.core.CoreFacade
import dev.rootshell.kai.core.ffi.ApprovalView
import dev.rootshell.kai.core.ffi.CallStart
import dev.rootshell.kai.core.ffi.CaptionLine
import dev.rootshell.kai.core.ffi.ConnectionView
import dev.rootshell.kai.core.ffi.ConversationView
import dev.rootshell.kai.core.ffi.MessageView
import dev.rootshell.kai.core.ffi.NotificationId
import dev.rootshell.kai.core.ffi.NotificationView
import dev.rootshell.kai.core.ffi.Update
import dev.rootshell.kai.core.ffi.VoiceView
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.channels.Channel
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.receiveAsFlow
import kotlinx.coroutines.isActive
import kotlinx.coroutines.launch

/** What the hub does with an update besides holding state: the platform's side. */
interface HubSink {
    fun notify(notification: NotificationView)
    fun withdraw(id: NotificationId)
    fun wakeAt(bootMs: ULong?)

    /** The connection's view changed; `previous` is the last one (null at start). */
    fun connection(previous: ConnectionView?, view: ConnectionView, visible: Boolean)

    /** A one-time sentence while the app is not on screen (on screen it is a snackbar). */
    fun notice(text: String)

    /** The call's view changed (design §7.1); `previous` is the last one (null at start). */
    fun voice(previous: VoiceView?, view: VoiceView, visible: Boolean)
}

/**
 * The one collector of the core's updates for the process's life (design §3.1, M4: pulled into a
 * Flow): the screens draw from its state, so an activity a fold recreates finds everything where it
 * was; notifications, the alarm and Kai's service follow from it through [HubSink]. It holds no
 * protocol rule: what notifies, when to retry and what a state says are the core's.
 */
class CoreHub(private val core: CoreFacade, private val sink: HubSink) {
    private val _connection = MutableStateFlow<ConnectionView?>(null)
    private val _conversation = MutableStateFlow<ConversationView?>(null)
    private val _messages = MutableStateFlow<List<MessageView>>(emptyList())
    private val _approvals = MutableStateFlow<List<ApprovalView>>(emptyList())
    private val _voice = MutableStateFlow<VoiceView?>(null)
    private val _captions = MutableStateFlow<List<CaptionLine>>(emptyList())
    /** Unbounded: a notice waits for the screen's snackbar, it is never dropped. */
    private val _notices = Channel<String>(Channel.UNLIMITED)

    val connection: StateFlow<ConnectionView?> = _connection
    val conversation: StateFlow<ConversationView?> = _conversation
    val messages: StateFlow<List<MessageView>> = _messages
    val approvals: StateFlow<List<ApprovalView>> = _approvals

    /** The voice call's snapshot (design §7.1); null before the core published one. */
    val voice: StateFlow<VoiceView?> = _voice

    /** The current call's captions, in order (design §7.6). */
    val captions: StateFlow<List<CaptionLine>> = _captions

    /**
     * A call asked for from outside a screen (a voice notice's Take over), started by the voice
     * screen: a visible activity may start the microphone (design §12.3, M16).
     */
    val pendingCall = MutableStateFlow<CallStart?>(null)

    /** Why the last call did not start (the core's words, or libwebrtc's), until the next try. */
    val callError = MutableStateFlow<String?>(null)

    /** Sentences for the screen (a snackbar) while the app is visible, each shown once. */
    val notices: Flow<String> = _notices.receiveAsFlow()

    /** A pairing link waiting for the owner's confirmation (a tap, a scan, a paste), in memory only. */
    val pairingText = MutableStateFlow("")

    /** A tapped approval or reply asks for the conversation (on the cover screen, its sheet). */
    val openConversation = MutableStateFlow(false)

    /**
     * A route asked for from inside the process (a tapped `lmgw-pair:` link, a notification's tap
     * through its trampoline), consumed by the navigation.
     */
    val pendingRoute = MutableStateFlow<String?>(null)

    /** [visible] as a flow: the assistant overlay's Expand waits for the app to show. */
    private val _visible = MutableStateFlow(false)
    val visibleState: StateFlow<Boolean> = _visible

    @Volatile
    var visible = false
        private set

    /** Starts collecting on `scope`. */
    fun start(scope: CoroutineScope) {
        scope.launch {
            while (isActive) apply(core.nextUpdate())
        }
    }

    fun setVisible(visible: Boolean) {
        if (this.visible == visible) return
        this.visible = visible
        _visible.value = visible
        core.setForeground(visible)
        _connection.value?.let { if (visible) sink.connection(it, it, true) }
    }

    fun apply(update: Update) {
        when (update) {
            is Update.Connection -> {
                val previous = _connection.value
                _connection.value = update.view
                sink.connection(previous, update.view, visible)
            }
            is Update.Conversation -> _conversation.value = update.view
            is Update.Messages -> _messages.value = MessageList.apply(_messages.value, update.delta)
            is Update.Approvals -> _approvals.value = update.open
            is Update.Notify -> sink.notify(update.notification)
            is Update.Withdraw -> sink.withdraw(update.id)
            is Update.WakeAt -> sink.wakeAt(update.bootMs)
            is Update.Notice -> if (visible) _notices.trySend(update.text) else sink.notice(update.text)
            is Update.Voice -> {
                val previous = _voice.value
                _voice.value = update.view
                sink.voice(previous, update.view, visible)
            }
            is Update.Caption -> _captions.value = CaptionList.apply(_captions.value, update.line)
        }
    }
}
