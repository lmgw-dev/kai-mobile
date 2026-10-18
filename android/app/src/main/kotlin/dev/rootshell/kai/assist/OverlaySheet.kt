package dev.rootshell.kai.assist

import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.gestures.Orientation
import androidx.compose.foundation.gestures.draggable
import androidx.compose.foundation.gestures.rememberDraggableState
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.activity.compose.PredictiveBackHandler
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.WindowInsets
import androidx.compose.foundation.layout.WindowInsetsSides
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.offset
import androidx.compose.foundation.layout.only
import androidx.compose.foundation.layout.safeDrawing
import androidx.compose.foundation.layout.windowInsetsPadding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.Button
import androidx.compose.material3.ButtonDefaults
import androidx.compose.material3.FilledTonalButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableFloatStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.IntOffset
import androidx.compose.ui.unit.dp
import dev.rootshell.kai.KaiApplication
import dev.rootshell.kai.R
import dev.rootshell.kai.core.ffi.ApprovalView
import dev.rootshell.kai.core.ffi.CallPhase
import dev.rootshell.kai.core.ffi.CallStart
import dev.rootshell.kai.core.ffi.CaptionLine
import dev.rootshell.kai.core.ffi.RingState
import dev.rootshell.kai.core.ffi.Speaker
import dev.rootshell.kai.core.ffi.VoiceView
import dev.rootshell.kai.ui.approvals.ApprovalCard
import dev.rootshell.kai.ui.approvals.ApprovalDecider
import dev.rootshell.kai.ui.approvals.ApprovalRules
import dev.rootshell.kai.ui.approvals.rememberApprovalDecider
import dev.rootshell.kai.ui.voice.ButtonRow
import dev.rootshell.kai.ui.voice.CallFacts
import dev.rootshell.kai.ui.voice.EndCard
import dev.rootshell.kai.ui.voice.Line
import dev.rootshell.kai.ui.voice.Ring
import dev.rootshell.kai.ui.voice.RingMotion
import dev.rootshell.kai.ui.voice.rememberFeel
import dev.rootshell.kai.ui.voice.rememberPushToTalkClicks
import dev.rootshell.kai.ui.voice.ringColour
import dev.rootshell.kai.ui.voice.ringDescription
import dev.rootshell.kai.ui.voice.ringGestures
import dev.rootshell.kai.ui.voice.ringMotion
import dev.rootshell.kai.ui.voice.voiceColours
import kotlin.coroutines.cancellation.CancellationException
import kotlin.math.roundToInt

/** What the sheet's controls do ([KaiSession] carries them out). */
interface OverlayActions {
    /** Back, a swipe down, a tap outside: `hide()`, whose dismissal the core decides on. */
    fun dismiss()

    /** Into the app's voice screen with the call (§9.2). */
    fun expand()

    /** Into the app's conversation, to type (§9.2's Keyboard). */
    fun keyboard()

    /** Open Kai for what the overlay cannot do: the microphone's permission, the link's state. */
    fun openKai(why: Blocked)

    /** The phone is locked: ask for the unlock, then talk in the app (§9.6). */
    fun unlock()

    /** A new call from the overlay (Talk, Reconnect, Take over after an end). */
    fun talk(start: CallStart)

    /** End: the call ends and the overlay goes. */
    fun end()
}

/**
 * The assistant overlay (design §9.2, §10.6): a bottom sheet with the ring, the state line, the last
 * two caption lines and a row of Mute and End, then Keyboard and Expand; "Voice is on <by>" with
 * Take over above the row while a bind is pending, and a call waiting for a decision as a card above
 * the controls (§11). The cover screen's width, or Material's maximum sheet width centred on the
 * inner screen, so the controls stay within one thumb's reach. A swipe down, a tap outside or back
 * dismisses it; the back gesture moves the sheet down as it goes (predictive back) and a cancelled
 * one brings it back. Locked, it shows Kai's name and the unlock only (§9.6), "Kai needs your
 * approval" when a call waits (no tool, no argument), and a running call's Mute and End without its
 * words, as the lock screen's notification does (§12.3). Content taller than the window (a large
 * font) scrolls.
 */
@Composable
fun OverlaySheet(
    app: KaiApplication,
    face: Face,
    view: VoiceView?,
    captions: List<CaptionLine>,
    approvals: List<ApprovalView>,
    hint: Int?,
    error: String?,
    act: OverlayActions,
) {
    val density = LocalDensity.current
    val threshold = with(density) { DISMISS_DRAG.toPx() }
    val backTravel = with(density) { BACK_TRAVEL.toPx() }
    var drag by remember { mutableFloatStateOf(0f) }
    val decider = rememberApprovalDecider(app)
    PredictiveBackHandler { progress ->
        try {
            progress.collect { e -> drag = e.progress * backTravel }
            act.dismiss()
        } catch (e: CancellationException) {
            drag = 0f
            throw e
        }
    }
    Box(
        Modifier
            .fillMaxSize()
            .background(Color.Black.copy(alpha = SCRIM))
            .clickable(interactionSource = remember { MutableInteractionSource() }, indication = null) { act.dismiss() },
        contentAlignment = Alignment.BottomCenter,
    ) {
        Surface(
            Modifier
                .widthIn(max = SHEET_MAX)
                .fillMaxWidth()
                .offset { IntOffset(0, drag.roundToInt()) }
                .draggable(
                    orientation = Orientation.Vertical,
                    state = rememberDraggableState { d -> drag = (drag + d).coerceAtLeast(0f) },
                    onDragStopped = { if (drag > threshold) act.dismiss() else drag = 0f },
                )
                // A tap on the sheet is not one outside it.
                .clickable(interactionSource = remember { MutableInteractionSource() }, indication = null) {},
            shape = RoundedCornerShape(topStart = 28.dp, topEnd = 28.dp),
            color = MaterialTheme.colorScheme.surfaceContainer,
            tonalElevation = 3.dp,
        ) {
            val scroll = rememberScrollState()
            Column(
                Modifier
                    .windowInsetsPadding(WindowInsets.safeDrawing.only(WindowInsetsSides.Horizontal + WindowInsetsSides.Bottom))
                    // Scrolls only when it must, so a swipe down still drags the sheet otherwise.
                    .verticalScroll(scroll, enabled = scroll.maxValue > 0)
                    .padding(start = 24.dp, end = 24.dp, top = 10.dp, bottom = 16.dp),
                horizontalAlignment = Alignment.CenterHorizontally,
            ) {
                Box(
                    Modifier
                        .size(width = 32.dp, height = 4.dp)
                        .background(MaterialTheme.colorScheme.onSurfaceVariant.copy(alpha = 0.4f), RoundedCornerShape(2.dp)),
                )
                when (face) {
                    is Face.Locked -> LockedFace(app, face, muted = view?.muted == true, asks = approvals.isNotEmpty(), act)
                    is Face.Blocked -> {
                        BlockedFace(face.why, act)
                        OverlayCard(approvals, decider, view)
                    }
                    Face.Call -> view?.let { CallFace(app, it, captions, hint, act) { OverlayCard(approvals, decider, view) } }
                    is Face.Ended -> {
                        OverlayRing(app, null)
                        OverlayCard(approvals, decider, view)
                        Spacer12()
                        EndCard(face.end) { act.talk(it) }
                    }
                    Face.Starting -> {
                        OverlayRing(app, null, connecting = true)
                        Words(stringResource(R.string.voice_state_starting))
                        OverlayCard(approvals, decider, view)
                    }
                }
                error?.let { Line(it, warning = true) }
            }
        }
    }
}

/** The first call that waits for a decision, with its place among them (§11). */
@Composable
private fun OverlayCard(approvals: List<ApprovalView>, decider: ApprovalDecider, view: VoiceView?) {
    val pick = ApprovalRules.pick(approvals) ?: return
    ApprovalCard(
        pick.card,
        decider,
        Modifier.padding(top = 10.dp),
        voice = true,
        announce = ApprovalRules.announces(pick.card, view),
        position = pick.position to pick.count,
        argumentsMax = 120.dp,
    )
}

@Composable
private fun CallFace(
    app: KaiApplication,
    v: VoiceView,
    captions: List<CaptionLine>,
    hint: Int?,
    act: OverlayActions,
    card: @Composable () -> Unit,
) {
    val voice = app.core.voice
    OverlayRing(app, v, onTalk = { act.talk(CallStart(new = false, takeOver = false)) })
    Words(v.words)
    CallFacts(v)
    // The last two lines of the call's captions (§7.6); the voice screen scrolls them all.
    val last = captions.filter { it.call == v.call }.takeLast(2)
    if (last.isNotEmpty()) {
        Column(Modifier.fillMaxWidth().padding(top = 10.dp), verticalArrangement = Arrangement.spacedBy(4.dp)) {
            last.forEach { l ->
                val who = when (l.speaker) {
                    Speaker.OWNER -> stringResource(R.string.caption_owner)
                    Speaker.KAI -> stringResource(R.string.caption_kai)
                    Speaker.NOTE -> stringResource(R.string.caption_note)
                }
                Text(
                    "$who: ${l.text}",
                    style = MaterialTheme.typography.bodyMedium,
                    color = if (l.speaker == Speaker.NOTE) MaterialTheme.colorScheme.error else MaterialTheme.colorScheme.onSurface,
                    maxLines = 2,
                    overflow = TextOverflow.Ellipsis,
                )
            }
        }
    }
    card()
    if (v.phase == CallPhase.PENDING) {
        v.pendingNote?.let { Line(it, warning = true) }
        ButtonRow(Modifier.padding(top = 12.dp)) {
            Button(onClick = { voice.takeOver() }, enabled = !v.takingOver) { Text(stringResource(R.string.take_over)) }
            EndButton(enabled = true) { act.end() }
        }
    } else {
        val ending = v.phase == CallPhase.ENDING
        ButtonRow(Modifier.padding(top = 12.dp)) {
            if (v.muted) {
                Button(onClick = { voice.setMuted(false) }, enabled = !ending) { Text(stringResource(R.string.unmute)) }
            } else {
                FilledTonalButton(onClick = { voice.setMuted(true) }, enabled = !ending) { Text(stringResource(R.string.mute)) }
            }
            EndButton(enabled = !ending) { act.end() }
        }
    }
    ButtonRow(Modifier.padding(top = 4.dp)) {
        TextButton(onClick = { act.keyboard() }) { Text(stringResource(R.string.overlay_keyboard)) }
        TextButton(onClick = { act.expand() }) { Text(stringResource(R.string.overlay_expand)) }
    }
    hint?.let { Line(stringResource(it)) }
}

/**
 * Locked (§9.6): Kai's name, the unlock, a running call's Mute and End without its words, and, when a
 * call waits for a decision, the notification's content-free words for it: no tool, no argument, and
 * deciding waits for the unlock (§11, §12.3).
 */
@Composable
private fun LockedFace(app: KaiApplication, face: Face.Locked, muted: Boolean, asks: Boolean, act: OverlayActions) {
    val voice = app.core.voice
    Words(stringResource(if (face.call) R.string.public_call else R.string.app_name))
    Text(
        stringResource(R.string.overlay_locked),
        style = MaterialTheme.typography.bodyMedium,
        textAlign = TextAlign.Center,
        modifier = Modifier.padding(top = 6.dp),
    )
    if (asks) {
        Text(
            stringResource(R.string.public_approval),
            style = MaterialTheme.typography.bodyMedium,
            textAlign = TextAlign.Center,
            modifier = Modifier.padding(top = 6.dp),
        )
    }
    ButtonRow(Modifier.padding(top = 12.dp)) {
        if (face.call) {
            if (muted) {
                Button(onClick = { voice.setMuted(false) }) { Text(stringResource(R.string.unmute)) }
            } else {
                FilledTonalButton(onClick = { voice.setMuted(true) }) { Text(stringResource(R.string.mute)) }
            }
            EndButton(enabled = true) { act.end() }
        }
        Button(onClick = { act.unlock() }) { Text(stringResource(R.string.overlay_unlock)) }
    }
}

/** No call started: why, with Open Kai. */
@Composable
private fun BlockedFace(why: Blocked, act: OverlayActions) {
    Words(
        when (why) {
            Blocked.Microphone -> stringResource(R.string.overlay_needs_microphone)
            is Blocked.Words -> why.words
            Blocked.Locked -> stringResource(R.string.overlay_locked)
        },
    )
    Button(onClick = { act.openKai(why) }, modifier = Modifier.padding(top = 12.dp)) {
        Text(stringResource(R.string.overlay_open_kai))
    }
}

/** The ring as the voice screen draws it (§10.3), smaller; a tap or hold acts as there. */
@Composable
private fun OverlayRing(app: KaiApplication, v: VoiceView?, connecting: Boolean = false, onTalk: () -> Unit = {}) {
    val colours = voiceColours()
    val ring = v?.ring ?: if (connecting) RingState.CONNECTING else RingState.REST
    val feel = rememberFeel(app)
    val labels = stringResource(R.string.ring_talk) to stringResource(R.string.ring_tap)
    val clicks = rememberPushToTalkClicks(app, v, feel)
    val words = v?.words.orEmpty()
    Ring(
        colour = ringColour(ring, colours),
        struck = ring == RingState.MUTED,
        motion = if (v != null || connecting) ringMotion(ring) else RingMotion.STATIC,
        levels = app.calls.levels,
        badge = if (v?.hold == true) colours.amber else null,
        description = stringResource(ringDescription(ring, waiting = v?.waitingFor != null)) + if (words.isEmpty()) "" else ". $words",
        modifier = Modifier
            .padding(top = 12.dp)
            .size(RING)
            .then(if (v != null) Modifier.ringGestures(app, v, feel, labels, clicks, onTalk) else Modifier),
    )
}

@Composable
private fun Words(text: String) {
    // Not a live region: the ring announces the state with its words.
    Text(
        text,
        style = MaterialTheme.typography.titleMedium,
        textAlign = TextAlign.Center,
        modifier = Modifier.padding(top = 10.dp),
    )
}

@Composable
private fun Spacer12() = Box(Modifier.size(12.dp))

@Composable
private fun EndButton(enabled: Boolean, onClick: () -> Unit) {
    Button(
        onClick = onClick,
        enabled = enabled,
        colors = ButtonDefaults.buttonColors(
            containerColor = MaterialTheme.colorScheme.error,
            contentColor = MaterialTheme.colorScheme.onError,
        ),
    ) { Text(stringResource(R.string.end_call)) }
}

/** Material's maximum bottom sheet width: the inner screen keeps the sheet centred at it (§10.6). */
private val SHEET_MAX = 640.dp
private val RING = 112.dp

/** A swipe down past this dismisses the sheet. */
private val DISMISS_DRAG = 96.dp

/** How far the back gesture moves the sheet down at its end (predictive back). */
private val BACK_TRAVEL = 160.dp
private const val SCRIM = 0.32f
