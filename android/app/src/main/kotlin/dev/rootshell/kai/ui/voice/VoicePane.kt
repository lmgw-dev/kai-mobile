package dev.rootshell.kai.ui.voice

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.BoxWithConstraints
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ColumnScope
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.Button
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.min
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import dev.rootshell.kai.KaiApplication
import dev.rootshell.kai.R
import dev.rootshell.kai.app.BackgroundSetup
import dev.rootshell.kai.core.ffi.CallPhase
import dev.rootshell.kai.core.ffi.CallStart
import dev.rootshell.kai.core.ffi.ConnectionAction
import dev.rootshell.kai.core.ffi.ConnectionState
import dev.rootshell.kai.core.ffi.ConnectionView
import dev.rootshell.kai.core.ffi.RingState
import dev.rootshell.kai.ui.VoiceArrangement
import dev.rootshell.kai.ui.approvals.ApprovalCard
import dev.rootshell.kai.ui.approvals.ApprovalRules
import dev.rootshell.kai.ui.approvals.rememberApprovalDecider
import dev.rootshell.kai.ui.currentLayout
import dev.rootshell.kai.ui.screens.actionLabel
import dev.rootshell.kai.ui.voiceArrangement
import dev.rootshell.kai.voice.active
import java.text.DateFormat
import java.util.Date

/**
 * The ring's colour between calls, by the link's state (Kai's: graphite at rest, amber while it
 * waits, grey struck through when unreachable).
 */
private fun connectionColour(state: ConnectionState?, colours: VoiceColours): Color = when (state) {
    ConnectionState.WAITING_FOR_CONFIRMATION, ConnectionState.CONNECTING -> colours.amber
    null, ConnectionState.NOT_PAIRED, ConnectionState.CONNECTED -> colours.graphite
    else -> colours.grey
}

/** Unreachable, refused or lapsed: the ring is struck through (§10.3). */
private fun unreachable(state: ConnectionState?): Boolean = when (state) {
    null, ConnectionState.NOT_PAIRED, ConnectionState.CONNECTING, ConnectionState.WAITING_FOR_CONFIRMATION, ConnectionState.CONNECTED -> false
    else -> true
}

/**
 * The voice screen (design §10.1, §10.6), one pane: the ring in the upper part with the state line
 * and the call's facts, the captions filling the middle, the controls at the bottom within the
 * thumb's reach, on every width (a foldable's cover screen alone, its inner screen beside the
 * conversation). A window too short for that at the font scale (up to Android's 200 %) puts the ring
 * beside the rest when it is wider than tall (the cover screen in landscape) and scrolls as one
 * column otherwise (a split screen, a large font): [voiceArrangement]. Between calls the link's
 * state, its actions, the last call's end and Talk. On a compact window a call waiting for a
 * decision is a card above the controls (§11); on a wide one the conversation beside shows it.
 *
 * Nothing here holds the call: it draws from the hub's state and acts through the app's voice
 * controller, so the activity a fold or an unfold recreates shows the same call, captions and
 * route, mid-sentence too, and nothing reconnects (the factory's warm-up is skipped while a call
 * runs).
 */
@Composable
fun VoicePane(
    app: KaiApplication,
    connection: ConnectionView?,
    onAction: (ConnectionAction) -> Unit,
    modifier: Modifier = Modifier,
    onOpenConversation: (() -> Unit)?,
    onSetup: () -> Unit,
) {
    val view by app.hub.voice.collectAsStateWithLifecycle()
    val captions by app.hub.captions.collectAsStateWithLifecycle()
    val approvals by app.hub.approvals.collectAsStateWithLifecycle()
    val decider = rememberApprovalDecider(app)
    val pick = if (ApprovalRules.onVoicePane(currentLayout())) ApprovalRules.pick(approvals) else null
    val error by app.hub.callError.collectAsStateWithLifecycle()
    val v = view
    val inCall = v.active()
    val paired = connection?.paired == true
    val talk = rememberTalk(app)
    val feel = rememberFeel(app)
    // The factory and its device module ahead of a call (§7.2); no microphone opens. Not while a
    // call runs (a fold recreating the screen mid-call warms nothing).
    LaunchedEffect(paired) { if (paired && !app.hub.voice.value.active()) app.calls.prepare() }

    val words = if (inCall) v?.words.orEmpty() else connection?.words ?: stringResource(R.string.voice_state_starting)
    val ringState = if (inCall) v?.ring ?: RingState.REST else RingState.REST
    val colours = voiceColours()
    val colour = if (inCall) ringColour(ringState, colours) else connectionColour(connection?.state, colours)
    val struck = if (inCall) ringState == RingState.MUTED else unreachable(connection?.state)
    val badge = if (inCall) v?.hold == true else connection?.held == true
    // The screen's one live region (§10.3): the ring's state and the state line's words, so TalkBack
    // says a change once. Between calls the link's words, the ring's click label saying it starts a
    // call.
    val description = if (inCall || !paired) {
        stringResource(ringDescription(ringState, waiting = inCall && v?.waitingFor != null)) + ". " + words
    } else {
        words
    }
    val labels = stringResource(R.string.ring_talk) to stringResource(R.string.ring_tap)
    val clicks = rememberPushToTalkClicks(app, v, feel)
    val gestures = if (paired) {
        Modifier.ringGestures(app, v, feel, labels, clicks) { talk(CallStart(new = false, takeOver = false)) }
    } else {
        Modifier
    }
    val ring: @Composable (Dp) -> Unit = { size ->
        Ring(
            colour = colour,
            struck = struck,
            motion = if (inCall) ringMotion(ringState) else RingMotion.STATIC,
            levels = app.calls.levels,
            badge = if (badge) colours.amber else null,
            description = description,
            modifier = Modifier.size(size).then(gestures),
        )
    }
    val head: @Composable ColumnScope.() -> Unit = {
        // Not a live region: the ring announces these words with its state.
        Text(
            words,
            style = MaterialTheme.typography.titleMedium,
            textAlign = TextAlign.Center,
            modifier = Modifier.padding(top = 12.dp),
        )
        connection?.confirmCode?.let { code ->
            Text(code, style = MaterialTheme.typography.displaySmall, fontFamily = FontFamily.Monospace, modifier = Modifier.padding(top = 16.dp))
        }
        if (inCall && v != null) {
            CallFacts(v)
        } else {
            LinkFacts(connection)
        }
        error?.let { Line(it, warning = true) }
    }
    val bottom: @Composable ColumnScope.() -> Unit = {
        pick?.let { p ->
            ApprovalCard(
                p.card,
                decider,
                Modifier.widthIn(max = 480.dp).padding(top = 8.dp),
                voice = true,
                announce = ApprovalRules.announces(p.card, v),
                position = p.position to p.count,
            )
        }
        if (inCall && v != null) {
            CallControls(app, v, feel)
        } else {
            if (paired) {
                val end = v?.end?.takeIf { v.phase == CallPhase.ENDED }
                if (end != null) {
                    EndCard(end, talk)
                } else {
                    Button(onClick = { talk(CallStart(new = false, takeOver = false)) }, modifier = Modifier.padding(top = 8.dp)) {
                        Text(stringResource(R.string.talk))
                    }
                }
            }
            LinkActions(connection, onAction, onSetup)
        }
        if (onOpenConversation != null && paired) {
            TextButton(onClick = onOpenConversation, modifier = Modifier.padding(top = 8.dp)) {
                Text(stringResource(R.string.open_conversation))
            }
        }
    }

    val fontScale = LocalDensity.current.fontScale
    BoxWithConstraints(modifier) {
        val height = maxHeight
        val width = maxWidth
        val arrangement = voiceArrangement(height.value, width.value, fontScale, card = pick != null)
        if (arrangement == VoiceArrangement.SIDE) {
            // Short and wide: the ring on the left, the rest scrolling beside it.
            Row(Modifier.fillMaxSize().padding(horizontal = 16.dp), verticalAlignment = Alignment.CenterVertically) {
                Column(Modifier.weight(0.4f).fillMaxHeight(), horizontalAlignment = Alignment.CenterHorizontally, verticalArrangement = Arrangement.Center) {
                    ring(min(height * 0.6f, 200.dp))
                }
                Column(
                    Modifier.weight(0.6f).fillMaxHeight().verticalScroll(rememberScrollState()).padding(vertical = 8.dp),
                    horizontalAlignment = Alignment.CenterHorizontally,
                ) {
                    head()
                    if (captions.isNotEmpty()) Captions(captions, Modifier.fillMaxWidth().heightIn(max = 240.dp).padding(top = 12.dp))
                    bottom()
                }
            }
        } else if (arrangement == VoiceArrangement.SCROLL) {
            // Short and narrow (a split screen): one column that scrolls, so the controls stay reachable.
            Column(
                Modifier.fillMaxSize().verticalScroll(rememberScrollState()).padding(horizontal = 16.dp, vertical = 8.dp),
                horizontalAlignment = Alignment.CenterHorizontally,
            ) {
                ring(RING_MIN)
                head()
                if (captions.isNotEmpty()) Captions(captions, Modifier.fillMaxWidth().heightIn(max = 240.dp).padding(top = 12.dp))
                bottom()
            }
        } else {
            val size = min(width * 0.6f, height * 0.3f).coerceIn(RING_MIN, RING_MAX)
            Column(
                Modifier.fillMaxSize().padding(horizontal = 24.dp).padding(top = 16.dp, bottom = 12.dp),
                horizontalAlignment = Alignment.CenterHorizontally,
            ) {
                ring(size)
                head()
                if (captions.isNotEmpty()) {
                    Captions(captions, Modifier.weight(1f).widthIn(max = 640.dp).fillMaxWidth().padding(top = 16.dp))
                } else {
                    Spacer(Modifier.weight(1f))
                }
                bottom()
            }
        }
    }
}

private val RING_MIN = 112.dp
private val RING_MAX = 240.dp

/** The link's facts between calls (design §5.3): the next try, the details, the hold, the certificate's notice. */
@Composable
private fun LinkFacts(connection: ConnectionView?) {
    connection?.nextTryAtMs?.let { at ->
        Line(stringResource(R.string.next_try, DateFormat.getTimeInstance(DateFormat.MEDIUM).format(Date(at))))
    }
    connection?.detail?.forEach { Line(it) }
    if (connection?.held == true) {
        Line(
            connection.holdFallback?.let { stringResource(R.string.hold_badge_fallback, it) } ?: stringResource(R.string.hold_badge),
            amber = true,
        )
    }
    connection?.certNotice?.let { Line(it, amber = true) }
}

/** The link's actions (design §5.3) and setup's background card once connected (§9.5). */
@Composable
private fun LinkActions(connection: ConnectionView?, onAction: (ConnectionAction) -> Unit, onSetup: () -> Unit) {
    val actions = connection?.actions.orEmpty()
    if (actions.isNotEmpty()) {
        // Wraps at a large font rather than running off a narrow screen.
        FlowRow(
            Modifier.padding(top = 12.dp),
            horizontalArrangement = Arrangement.spacedBy(8.dp, Alignment.CenterHorizontally),
            verticalArrangement = Arrangement.spacedBy(4.dp),
        ) {
            actions.forEachIndexed { i, action ->
                val label = stringResource(actionLabel(action))
                if (i == 0) {
                    Button(onClick = { onAction(action) }) { Text(label) }
                } else {
                    OutlinedButton(onClick = { onAction(action) }) { Text(label) }
                }
            }
        }
    }
    val context = LocalContext.current
    if (connection?.state == ConnectionState.CONNECTED && BackgroundSetup.open(BackgroundSetup.facts(context)).isNotEmpty()) {
        TextButton(onClick = onSetup, modifier = Modifier.padding(top = 4.dp)) {
            Text(stringResource(R.string.background_prompt))
        }
    }
}
