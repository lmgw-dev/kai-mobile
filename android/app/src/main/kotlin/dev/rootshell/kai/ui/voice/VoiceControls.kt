package dev.rootshell.kai.ui.voice

import android.Manifest
import android.content.pm.PackageManager
import android.view.HapticFeedbackConstants
import android.view.View
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.foundation.background
import androidx.compose.foundation.gestures.PressGestureScope
import androidx.compose.foundation.gestures.detectTapGestures
import androidx.compose.foundation.isSystemInDarkTheme
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.lazy.rememberLazyListState
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.selection.toggleable
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.ArrowDropDown
import androidx.compose.material.icons.filled.Check
import androidx.compose.material3.Button
import androidx.compose.material3.ButtonDefaults
import androidx.compose.material3.Card
import androidx.compose.material3.CardDefaults
import androidx.compose.material3.DropdownMenu
import androidx.compose.material3.DropdownMenuItem
import androidx.compose.material3.FilledTonalButton
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.Switch
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.Immutable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.saveable.Saver
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.runtime.snapshotFlow
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.LocalView
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.onClick
import androidx.compose.ui.semantics.role
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.core.content.ContextCompat
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import dev.rootshell.kai.KaiApplication
import dev.rootshell.kai.R
import dev.rootshell.kai.core.ffi.CallEnd
import dev.rootshell.kai.core.ffi.CallPhase
import dev.rootshell.kai.core.ffi.CallStart
import dev.rootshell.kai.core.ffi.CaptionLine
import dev.rootshell.kai.core.ffi.RingState
import dev.rootshell.kai.core.ffi.Speaker
import dev.rootshell.kai.core.ffi.VoiceAction
import dev.rootshell.kai.core.ffi.VoiceView
import dev.rootshell.kai.ui.ScrollFrameWatch
import dev.rootshell.kai.ui.conversation.followEnd
import dev.rootshell.kai.voice.Started
import dev.rootshell.kai.voice.active
import kotlinx.coroutines.launch

/**
 * Kai's state colours on the voice screen (design §10.2, §10.3): fixed whatever
 * `appearance.colours` says, so the ring keeps Kai's colours under the system's dynamic colour too;
 * a light and a dark set, each readable on Kai's background of its theme (on #EFF0F1 the ring's
 * colours reach 3:1 and the amber of small text 4.5:1; amber 4.9:1, blue 4.8:1, graphite 6.7:1,
 * grey 3.5:1). `onAmber` is the text on an amber fill.
 */
@Immutable
data class VoiceColours(val amber: Color, val onAmber: Color, val blue: Color, val graphite: Color, val grey: Color)

/** On Kai's graphite (#1D2125): Breeze's amber and Kai's blue. */
val VoiceDark = VoiceColours(
    amber = Color(0xFFF67400),
    onAmber = Color.Black,
    blue = Color(0xFF3DAEE9),
    graphite = Color(0xFFB4BBC2),
    grey = Color(0xFF6B737A),
)

/** On Kai's light grey (#EFF0F1): a darker amber and Kai's deep blue. */
val VoiceLight = VoiceColours(
    amber = Color(0xFFA84F00),
    onAmber = Color.White,
    blue = Color(0xFF1B6EA8),
    graphite = Color(0xFF4D545B),
    grey = Color(0xFF7A8189),
)

/** The state colours of the theme the screen shows (the same rule as KaiTheme's). */
@Composable
fun voiceColours(): VoiceColours = if (isSystemInDarkTheme()) VoiceDark else VoiceLight

/**
 * The ring's colour for a call's state (design §10.3): amber listening and interrupted, blue
 * thinking and speaking, grey (struck through) muted, graphite otherwise.
 */
fun ringColour(ring: RingState, colours: VoiceColours): Color = when (ring) {
    RingState.LISTENING, RingState.INTERRUPTED -> colours.amber
    RingState.THINKING, RingState.SPEAKING -> colours.blue
    RingState.MUTED -> colours.grey
    RingState.REST, RingState.IDLE, RingState.PENDING, RingState.CONNECTING -> colours.graphite
}

/** What the ring's gestures do now. */
private enum class RingGesture { TALK, HOLD, TAP }

private fun VoiceView?.ringGesture(): RingGesture = when {
    !active() -> RingGesture.TALK
    this?.pushToTalk == true && (phase == CallPhase.LIVE || phase == CallPhase.CONNECTING) -> RingGesture.HOLD
    else -> RingGesture.TAP
}

/**
 * The push-to-talk gesture's haptics, the confirm haptic for a verdict the core took and the reject
 * haptic (design §10.5), as `voice.haptics` allows;
 * `View.performHapticFeedback` honours the system's haptics setting. Posted to the view's thread, so
 * a start answered on another thread may feel too.
 */
class Feel(private val view: View, private val on: Boolean) {
    fun start() = feel(HapticFeedbackConstants.GESTURE_START)
    fun end() = feel(HapticFeedbackConstants.GESTURE_END)
    fun reject() = feel(HapticFeedbackConstants.REJECT)

    /** A decision the core took (an approval's verdict, §11). */
    fun confirm() = feel(HapticFeedbackConstants.CONFIRM)

    private fun feel(what: Int) {
        if (on) view.post { view.performHapticFeedback(what) }
    }
}

@Composable
fun rememberFeel(app: KaiApplication): Feel {
    val view = LocalView.current
    val on = remember { app.core.settings().voice.haptics }
    return remember(view, on) { Feel(view, on) }
}

/**
 * The ring's gestures for a call (design §7.5, §7.6): with no call a tap starts one; in
 * push-to-talk a press holds the turn; in automatic mode a tap stops Kai, or mutes when nothing is
 * open (kai K6). Keyed by the call and what the gesture does, never by the phase, so the bind
 * (connecting to live) does not restart a press that is held. A tap's action is TalkBack's click;
 * in push-to-talk TalkBack's click starts the turn and the next one sends it ([clicks],
 * [pushToTalkClick]), since a held press is hard to make through TalkBack.
 */
fun Modifier.ringGestures(
    app: KaiApplication,
    view: VoiceView?,
    feel: Feel,
    labels: Pair<String, String>,
    clicks: PushToTalkClicks? = null,
    onTalk: () -> Unit,
): Modifier {
    val gesture = view.ringGesture()
    val voice = app.core.voice
    val click: Modifier = when (gesture) {
        RingGesture.TALK -> Modifier.semantics {
            onClick(labels.first) {
                onTalk()
                true
            }
        }
        RingGesture.TAP -> Modifier.semantics {
            onClick(labels.second) {
                voice.tap()
                true
            }
        }
        RingGesture.HOLD -> clicks?.let { Modifier.pushToTalkClick(app, view, feel, it) } ?: Modifier
    }
    return then(click).pointerInput(view?.call, gesture) {
        when (gesture) {
            RingGesture.TALK -> detectTapGestures(onTap = { onTalk() })
            RingGesture.HOLD -> detectTapGestures(onPress = { holdToTalk(app, feel) })
            RingGesture.TAP -> detectTapGestures(onTap = { voice.tap() })
        }
    }
}

/**
 * Push-to-talk as TalkBack's click on one control (design §7.5): its labels, and whether a click of
 * it started the turn held now (main thread), so the control releases that turn when it leaves
 * ([rememberPushToTalkClicks]).
 */
class PushToTalkClicks(val start: String, val send: String) {
    var started = false
}

/**
 * A control's [PushToTalkClicks]. A turn its click started is released, with the gesture's end
 * haptic, when the control leaves the composition or the call or its gesture changes (the call
 * ended, the mode changed, a fold recreated the activity), as the held gesture's `finally` releases
 * a press, so a turn is never left held open; the core ignores a release with no turn held.
 */
@Composable
fun rememberPushToTalkClicks(app: KaiApplication, view: VoiceView?, feel: Feel): PushToTalkClicks {
    val start = stringResource(R.string.ptt_click_start)
    val send = stringResource(R.string.ptt_click_send)
    val clicks = remember(start, send) { PushToTalkClicks(start, send) }
    val holds = view.ringGesture() == RingGesture.HOLD
    DisposableEffect(clicks, view?.call, holds) {
        onDispose {
            if (clicks.started) {
                clicks.started = false
                app.core.voice.release()
                feel.end()
            }
        }
    }
    return clicks
}

/**
 * Push-to-talk as TalkBack's click (design §7.5, the accessibility audit): the first click starts
 * the turn, the next sends it, labelled for what it does now. Muted, the click is refused with the
 * reject haptic, as a press is. [clicks] remembers a turn the click started, for its release.
 */
fun Modifier.pushToTalkClick(app: KaiApplication, view: VoiceView?, feel: Feel, clicks: PushToTalkClicks): Modifier {
    val pressed = view?.pressed == true
    val voice = app.core.voice
    return semantics {
        role = Role.Button
        onClick(if (pressed) clicks.send else clicks.start) {
            when {
                pressed -> {
                    clicks.started = false
                    voice.release()
                    feel.end()
                }
                app.hub.voice.value?.muted == true -> feel.reject()
                else -> {
                    clicks.started = true
                    voice.press()
                    feel.start()
                }
            }
            true
        }
    }
}

/**
 * One push-to-talk press (design §7.5): the release always follows the press it started, also when
 * the gesture is cancelled before the finger lifts (the call ended, the mode changed, a fold
 * recreated the activity), so a turn is never left held open. Muted, the core ignores a press, so
 * the press is refused with the reject haptic instead of the gesture's start (§10.5).
 */
private suspend fun PressGestureScope.holdToTalk(app: KaiApplication, feel: Feel) {
    if (app.hub.voice.value?.muted == true) {
        feel.reject()
        return
    }
    val voice = app.core.voice
    voice.press()
    feel.start()
    try {
        tryAwaitRelease()
    } finally {
        voice.release()
        feel.end()
    }
}

/** A [CallStart] kept across a fold's recreation while the microphone's permission is asked. */
private val CallStartSaver = Saver<CallStart?, List<Boolean>>(
    save = { s -> s?.let { listOf(it.new, it.takeOver) } },
    restore = { CallStart(new = it[0], takeOver = it[1]) },
)

/**
 * Starts calls from the screen (design §7.3): the microphone permission first, the start on the
 * process's scope (a fold recreates the activity mid-start), a notice's Take over picked up. The
 * call waiting for the permission survives a fold (the dialog's answer reaches the recreated
 * screen); a start refused at once gets the reject haptic.
 */
@Composable
fun rememberTalk(app: KaiApplication): (CallStart) -> Unit {
    val context = LocalContext.current
    val feel = rememberFeel(app)
    var waiting by rememberSaveable(stateSaver = CallStartSaver) { mutableStateOf<CallStart?>(null) }
    val start: (CallStart) -> Unit = { s ->
        app.hub.callError.value = null
        app.scope.launch {
            val why = (app.calls.start(s) as? Started.Refused)?.words
            app.hub.callError.value = why
            if (why != null) feel.reject()
        }
    }
    val ask = rememberLauncherForActivityResult(ActivityResultContracts.RequestPermission()) { granted ->
        val s = waiting
        waiting = null
        if (granted && s != null) {
            start(s)
        } else if (!granted) {
            app.hub.callError.value = app.getString(R.string.voice_needs_microphone)
        }
    }
    val talk: (CallStart) -> Unit = { s ->
        if (ContextCompat.checkSelfPermission(context, Manifest.permission.RECORD_AUDIO) == PackageManager.PERMISSION_GRANTED) {
            start(s)
        } else {
            waiting = s
            ask.launch(Manifest.permission.RECORD_AUDIO)
        }
    }
    val pending by app.hub.pendingCall.collectAsStateWithLifecycle()
    LaunchedEffect(pending) {
        val s = pending ?: return@LaunchedEffect
        app.hub.pendingCall.value = null
        if (!app.hub.voice.value.active()) talk(s)
    }
    return talk
}

/**
 * A call's facts under its state line (design §7.1, §7.10, §8.2): half duplex, K16's wait, Kai's
 * local mute, a held stage, the hold, a static value's note, lmgw's latest error.
 */
@Composable
fun CallFacts(v: VoiceView) {
    Column(horizontalAlignment = Alignment.CenterHorizontally) {
        if (v.halfDuplex) Line(stringResource(R.string.voice_half_duplex))
        v.waitingFor?.let { Line(stringResource(R.string.voice_waiting_for, it)) }
        if (v.kaiMuted) Line(stringResource(R.string.voice_kai_muted))
        v.heldStage?.let { Line(stringResource(R.string.voice_held_stage, it), amber = true) }
        if (v.hold) {
            Line(
                v.holdFallback?.let { stringResource(R.string.hold_badge_fallback, it) } ?: stringResource(R.string.hold_badge),
                amber = true,
            )
        }
        v.note?.let { Line(it) }
        v.error?.let { Line(it, warning = true) }
    }
}

/**
 * A running call's controls (design §10.1): a pending bind's prompt with Take over and End (§7.9);
 * otherwise push-to-talk's hold button, then Mute, the route and End in one row, and the mode switch
 * with Stop under it.
 */
@Composable
fun CallControls(app: KaiApplication, v: VoiceView, feel: Feel) {
    val voice = app.core.voice
    Column(Modifier.fillMaxWidth(), horizontalAlignment = Alignment.CenterHorizontally) {
        if (v.phase == CallPhase.PENDING) {
            Card(
                colors = CardDefaults.cardColors(containerColor = MaterialTheme.colorScheme.surfaceContainerHigh),
                modifier = Modifier.widthIn(max = 480.dp).fillMaxWidth(),
            ) {
                Column(Modifier.padding(16.dp).fillMaxWidth(), horizontalAlignment = Alignment.CenterHorizontally) {
                    v.pendingNote?.let { Line(it, warning = true) }
                    ButtonRow(Modifier.padding(top = 8.dp)) {
                        Button(onClick = { voice.takeOver() }, enabled = !v.takingOver) { Text(stringResource(R.string.take_over)) }
                        EndButton(onClick = { voice.end() }, enabled = true)
                    }
                }
            }
            return@Column
        }
        val live = v.phase == CallPhase.LIVE || v.phase == CallPhase.CONNECTING
        if (v.pushToTalk && live) HoldToTalk(app, v, feel)
        ButtonRow(Modifier.padding(top = 12.dp)) {
            val ending = v.phase == CallPhase.ENDING
            if (v.muted) {
                Button(onClick = { voice.setMuted(false) }, enabled = !ending) { Text(stringResource(R.string.unmute)) }
            } else {
                FilledTonalButton(onClick = { voice.setMuted(true) }, enabled = !ending) { Text(stringResource(R.string.mute)) }
            }
            RouteButton(app, enabled = !ending)
            EndButton(onClick = { voice.end() }, enabled = !ending)
        }
        ButtonRow(Modifier.padding(top = 4.dp)) {
            // The label and the switch are one control for TalkBack ("Push-to-talk, on").
            Row(
                Modifier.toggleable(value = v.pushToTalk, enabled = live, role = Role.Switch) { voice.setPushToTalk(it) },
                verticalAlignment = Alignment.CenterVertically,
                horizontalArrangement = Arrangement.spacedBy(12.dp),
            ) {
                Text(stringResource(R.string.push_to_talk), style = MaterialTheme.typography.labelLarge)
                Switch(checked = v.pushToTalk, onCheckedChange = null, enabled = live)
            }
            if (v.stoppable) OutlinedButton(onClick = { voice.stop() }) { Text(stringResource(R.string.stop_kai)) }
        }
    }
}

@Composable
private fun EndButton(onClick: () -> Unit, enabled: Boolean) {
    Button(
        onClick = onClick,
        enabled = enabled,
        colors = ButtonDefaults.buttonColors(
            containerColor = MaterialTheme.colorScheme.error,
            contentColor = MaterialTheme.colorScheme.onError,
        ),
    ) { Text(stringResource(R.string.end_call)) }
}

/**
 * The route button (design §8.3): the device the call plays on, with "switching…" while Android
 * sets a Bluetooth headset's link up; its menu lists the communication devices and sets one for
 * the call. Said under it: a device Android refused, where Android moved the call after it had
 * switched (until the next device change or choice), a device it did not switch to in time.
 */
@Composable
fun RouteButton(app: KaiApplication, enabled: Boolean, modifier: Modifier = Modifier) {
    val audio = app.voice.audio
    val routes by audio.routes.collectAsStateWithLifecycle()
    var open by rememberSaveable { mutableStateOf(false) }
    val label = routes.current?.let { audio.label(it) } ?: stringResource(R.string.route_unknown)
    Column(modifier, horizontalAlignment = Alignment.CenterHorizontally) {
        Box {
            OutlinedButton(onClick = { open = true }, enabled = enabled && routes.devices.isNotEmpty()) {
                Text(
                    if (routes.switching) stringResource(R.string.route_switching, label) else label,
                    maxLines = 1,
                    overflow = TextOverflow.Ellipsis,
                    modifier = Modifier.weight(1f, fill = false),
                )
                Icon(Icons.Default.ArrowDropDown, contentDescription = stringResource(R.string.route_title))
            }
            DropdownMenu(expanded = open && routes.devices.isNotEmpty(), onDismissRequest = { open = false }) {
                Text(
                    stringResource(R.string.route_title),
                    style = MaterialTheme.typography.labelLarge,
                    modifier = Modifier.padding(horizontal = 16.dp, vertical = 8.dp),
                )
                routes.devices.forEach { d ->
                    DropdownMenuItem(
                        text = { Text(audio.label(d)) },
                        onClick = {
                            open = false
                            app.voice.choose(d.id)
                        },
                        leadingIcon = {
                            if (d.id == routes.requested) {
                                Icon(Icons.Default.Check, contentDescription = null)
                            } else {
                                Box(Modifier.size(24.dp))
                            }
                        },
                    )
                }
            }
        }
        routes.refused?.let { Line(stringResource(R.string.route_refused, audio.label(it)), warning = true) }
        routes.left?.let { left ->
            val to = routes.routed
            Line(
                if (to != null) {
                    stringResource(R.string.route_moved, audio.label(to))
                } else {
                    stringResource(R.string.route_moved_off, audio.label(left))
                },
            )
        }
        routes.stalled?.let { Line(stringResource(R.string.route_stalled, audio.label(it)), warning = true) }
    }
}

/**
 * The push-to-talk button: held, it talks (design §7.5); TalkBack's click starts the turn and the
 * next one sends it. At least 64 dp high, taller when a large font needs it.
 */
@Composable
private fun HoldToTalk(app: KaiApplication, v: VoiceView, feel: Feel) {
    val label = stringResource(if (v.pressed) R.string.talking else R.string.hold_to_talk)
    val colours = voiceColours()
    val clicks = rememberPushToTalkClicks(app, v, feel)
    Box(
        Modifier
            .padding(top = 12.dp)
            .widthIn(max = 320.dp)
            .fillMaxWidth()
            .heightIn(min = 64.dp)
            .background(
                if (v.pressed) colours.amber else MaterialTheme.colorScheme.secondaryContainer,
                RoundedCornerShape(32.dp),
            )
            .pushToTalkClick(app, v, feel, clicks)
            .pointerInput(v.call) {
                detectTapGestures(onPress = { holdToTalk(app, feel) })
            }
            .padding(horizontal = 16.dp, vertical = 12.dp),
        contentAlignment = Alignment.Center,
    ) {
        Text(
            label,
            style = MaterialTheme.typography.titleMedium,
            color = if (v.pressed) colours.onAmber else MaterialTheme.colorScheme.onSecondaryContainer,
            textAlign = TextAlign.Center,
        )
    }
}

/** A row of buttons that wraps (centred) when a large font makes them wider than the screen. */
@Composable
fun ButtonRow(modifier: Modifier = Modifier, content: @Composable () -> Unit) {
    FlowRow(
        modifier,
        horizontalArrangement = Arrangement.spacedBy(8.dp, Alignment.CenterHorizontally),
        verticalArrangement = Arrangement.spacedBy(4.dp),
        itemVerticalAlignment = Alignment.CenterVertically,
    ) { content() }
}

/**
 * How the last call ended (design §7.7, §7.8): its words, lmgw's sentence under them, and Reconnect
 * or Take over where the end offers them; Talk beside them otherwise.
 */
@Composable
fun EndCard(end: CallEnd, talk: (CallStart) -> Unit) {
    Column(Modifier.fillMaxWidth(), horizontalAlignment = Alignment.CenterHorizontally) {
        end.words?.let { words ->
            Card(
                colors = CardDefaults.cardColors(containerColor = MaterialTheme.colorScheme.surfaceContainerHigh),
                modifier = Modifier.widthIn(max = 480.dp).fillMaxWidth(),
            ) {
                Column(Modifier.padding(16.dp)) {
                    Text(
                        stringResource(R.string.voice_last_call),
                        style = MaterialTheme.typography.labelMedium,
                        color = MaterialTheme.colorScheme.onSurfaceVariant,
                    )
                    Text(words, style = MaterialTheme.typography.bodyLarge, modifier = Modifier.padding(top = 4.dp))
                    end.message?.let { m ->
                        Text(
                            m,
                            style = MaterialTheme.typography.bodySmall,
                            color = MaterialTheme.colorScheme.onSurfaceVariant,
                            modifier = Modifier.padding(top = 4.dp),
                        )
                    }
                }
            }
        }
        ButtonRow(Modifier.padding(top = 12.dp)) {
            end.actions.forEach { a ->
                when (a) {
                    VoiceAction.RECONNECT -> Button(onClick = { talk(CallStart(new = false, takeOver = false)) }) {
                        Text(stringResource(R.string.reconnect))
                    }
                    VoiceAction.TAKE_OVER -> Button(onClick = { talk(CallStart(new = false, takeOver = true)) }) {
                        Text(stringResource(R.string.take_over))
                    }
                }
            }
            if (VoiceAction.RECONNECT !in end.actions) {
                if (end.actions.isEmpty()) {
                    Button(onClick = { talk(CallStart(new = false, takeOver = false)) }) { Text(stringResource(R.string.talk)) }
                } else {
                    OutlinedButton(onClick = { talk(CallStart(new = false, takeOver = false)) }) { Text(stringResource(R.string.talk)) }
                }
            }
        }
    }
}

@Composable
fun Line(text: String, warning: Boolean = false, amber: Boolean = false) {
    Text(
        text,
        style = MaterialTheme.typography.bodySmall,
        color = when {
            warning -> MaterialTheme.colorScheme.error
            amber -> voiceColours().amber
            else -> MaterialTheme.colorScheme.onSurfaceVariant
        },
        textAlign = TextAlign.Center,
        modifier = Modifier.padding(top = 6.dp),
    )
}

/**
 * The call's captions (design §7.6): the voice screen scrolls them, the newest at the bottom; the
 * owner's words, Kai's (as the thread kept them once `lmgw.chat.reply` came), lmgw's notes in the
 * error colour, a line still changing a little lighter.
 */
@Composable
fun Captions(lines: List<CaptionLine>, modifier: Modifier = Modifier) {
    val list = rememberLazyListState()
    val colours = voiceColours()
    // As the conversation: the newest line is followed only while the owner is at the end, and
    // again once another call's lines show.
    var follow by remember { mutableStateOf(true) }
    val call = lines.firstOrNull()?.call
    LaunchedEffect(call) { follow = true }
    LaunchedEffect(list) {
        snapshotFlow { list.isScrollInProgress }.collect { scrolling -> if (!scrolling) follow = !list.canScrollForward }
    }
    LaunchedEffect(lines.size, lines.lastOrNull()?.text, follow) {
        if (follow) followEnd(list)
    }
    ScrollFrameWatch(list, "captions")
    LazyColumn(modifier, state = list, verticalArrangement = Arrangement.spacedBy(10.dp)) {
        items(lines, key = { "${it.call}:${it.id}" }) { l ->
            val who = when (l.speaker) {
                Speaker.OWNER -> stringResource(R.string.caption_owner)
                Speaker.KAI -> stringResource(R.string.caption_kai)
                Speaker.NOTE -> stringResource(R.string.caption_note)
            }
            val dot = when (l.speaker) {
                Speaker.KAI -> colours.blue
                Speaker.OWNER -> colours.amber
                Speaker.NOTE -> MaterialTheme.colorScheme.error
            }
            // One TalkBack item per line: who, then the words.
            Row(Modifier.semantics(mergeDescendants = true) {}, verticalAlignment = Alignment.Top) {
                Box(Modifier.padding(top = 7.dp, end = 10.dp).size(8.dp).background(dot, CircleShape))
                Column {
                    Text(who, style = MaterialTheme.typography.labelMedium, color = MaterialTheme.colorScheme.onSurfaceVariant)
                    Text(
                        l.text,
                        style = MaterialTheme.typography.bodyLarge,
                        color = when {
                            l.speaker == Speaker.NOTE -> MaterialTheme.colorScheme.error
                            !l.done -> MaterialTheme.colorScheme.onSurface.copy(alpha = 0.75f)
                            else -> MaterialTheme.colorScheme.onSurface
                        },
                    )
                    l.note?.let {
                        Text(it, style = MaterialTheme.typography.labelSmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
                    }
                }
            }
        }
    }
}
