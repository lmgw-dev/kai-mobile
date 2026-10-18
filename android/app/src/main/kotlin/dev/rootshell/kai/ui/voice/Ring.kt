package dev.rootshell.kai.ui.voice

import android.content.Context
import android.database.ContentObserver
import android.os.Handler
import android.os.Looper
import android.provider.Settings
import androidx.compose.foundation.layout.Box
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.State
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableFloatStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.drawWithCache
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.StrokeCap
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.semantics.LiveRegionMode
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.liveRegion
import androidx.compose.ui.semantics.semantics
import androidx.compose.runtime.withFrameNanos
import dev.rootshell.kai.R
import dev.rootshell.kai.core.ffi.RingState
import dev.rootshell.kai.voice.Levels
import kotlin.math.PI
import kotlin.math.abs
import kotlin.math.sin

/** What moves on the ring (design §10.3). */
enum class RingMotion {
    /** One static frame: at rest, idle, muted, a pending prompt. */
    STATIC,

    /** The halo follows the microphone's level. */
    MIC,

    /** The halo follows Kai's level. */
    KAI,

    /** An arc runs round the ring while Kai thinks. */
    SPIN,

    /** The ring breathes while the call connects. */
    BREATHE,
}

/** What the ring does in a call's state: it animates only while the call is active (§10.3's cost). */
fun ringMotion(ring: RingState): RingMotion = when (ring) {
    RingState.LISTENING, RingState.INTERRUPTED -> RingMotion.MIC
    RingState.SPEAKING -> RingMotion.KAI
    RingState.THINKING -> RingMotion.SPIN
    RingState.CONNECTING -> RingMotion.BREATHE
    RingState.REST, RingState.IDLE, RingState.MUTED, RingState.PENDING -> RingMotion.STATIC
}

/**
 * The ring's words for TalkBack (§10.3: the state). `waiting`: in automatic mode another person's
 * turn runs in the thread (K16), which the ring shows as idle too.
 */
fun ringDescription(ring: RingState, waiting: Boolean = false): Int = when (ring) {
    RingState.REST -> R.string.ring_rest
    RingState.CONNECTING -> R.string.ring_connecting
    RingState.LISTENING -> R.string.ring_listening
    RingState.THINKING -> R.string.ring_thinking
    RingState.SPEAKING -> R.string.ring_speaking
    RingState.INTERRUPTED -> R.string.ring_interrupted
    RingState.IDLE -> if (waiting) R.string.ring_waiting else R.string.ring_idle
    RingState.MUTED -> R.string.ring_muted
    RingState.PENDING -> R.string.ring_pending
}

/** Animations off (the animator duration scale at 0): the ring shows its state's colour, still. */
fun reducedMotion(context: Context): Boolean =
    Settings.Global.getFloat(context.contentResolver, Settings.Global.ANIMATOR_DURATION_SCALE, 1f) == 0f

/** [reducedMotion], followed while the ring shows (the setting may change with the screen open). */
@Composable
private fun rememberReducedMotion(): State<Boolean> {
    val context = LocalContext.current
    val reduced = remember { mutableStateOf(reducedMotion(context)) }
    DisposableEffect(context) {
        val observer = object : ContentObserver(Handler(Looper.getMainLooper())) {
            override fun onChange(selfChange: Boolean) {
                reduced.value = reducedMotion(context)
            }
        }
        val uri = Settings.Global.getUriFor(Settings.Global.ANIMATOR_DURATION_SCALE)
        context.contentResolver.registerContentObserver(uri, false, observer)
        reduced.value = reducedMotion(context)
        onDispose { context.contentResolver.unregisterContentObserver(observer) }
    }
    return reduced
}

/**
 * The halo's level one frame on (§10.3): up fast, down slowly, as a level meter; null when it is
 * within [LEVEL_EPSILON] of `target` already, so a steady level (silence included) writes nothing
 * and draws nothing.
 */
fun nextLevel(level: Float, target: Float): Float? {
    if (abs(target - level) < LEVEL_EPSILON) return null
    val k = if (target > level) ATTACK else RELEASE
    val next = level + (target - level) * k
    return if (abs(target - next) < LEVEL_EPSILON) target else next
}

/**
 * The ring (design §10.3), a native Compose drawing: its state's colour, struck through when muted
 * or unreachable, a badge (`badge`, the hold's amber) for the hold, and while a call is active the
 * motion of its state: a halo following the microphone's level while listening and Kai's while it
 * speaks, an arc running while Kai thinks, a breath while connecting. The levels are read at the
 * frame rate and written only when the halo moves; the clock runs only for the arc and the breath,
 * and the draw phase reads only what its motion uses (no recomposition per frame), so a steady halo
 * (silence) draws nothing and the display may lower its refresh rate. At rest, idle and muted it
 * draws one static frame, as it does with animations off. `description` is the content
 * description, announced politely: the voice screen's one live region.
 */
@Composable
fun Ring(
    colour: Color,
    struck: Boolean,
    motion: RingMotion,
    levels: Levels?,
    badge: Color?,
    description: String,
    modifier: Modifier = Modifier,
) {
    val reduced by rememberReducedMotion()
    val animate = motion != RingMotion.STATIC && !reduced
    val halo = animate && (motion == RingMotion.MIC || motion == RingMotion.KAI)
    val clocked = animate && (motion == RingMotion.SPIN || motion == RingMotion.BREATHE)
    val level = remember { mutableFloatStateOf(0f) }
    val clock = remember { mutableFloatStateOf(0f) }
    LaunchedEffect(animate, motion) {
        level.floatValue = 0f
        if (!animate) return@LaunchedEffect
        var last = 0L
        while (true) {
            withFrameNanos { t ->
                if (halo) {
                    val target = when (motion) {
                        RingMotion.MIC -> levels?.mic ?: 0f
                        else -> levels?.kai ?: 0f
                    }
                    nextLevel(level.floatValue, target)?.let { level.floatValue = it }
                } else if (clocked) {
                    val dt = if (last == 0L) 0f else (t - last) / 1e9f
                    last = t
                    clock.floatValue = (clock.floatValue + dt) % CLOCK_WRAP_S
                }
            }
        }
    }
    Box(
        modifier
            .semantics {
                contentDescription = description
                liveRegion = LiveRegionMode.Polite
            }
            .drawWithCache {
                val stroke = size.minDimension * 0.06f
                // Room around the ring for the level's halo.
                val radius = (size.minDimension / 2 - stroke) * 0.8f
                val arcBox = Size(radius * 2, radius * 2)
                onDrawBehind {
                    val c = center
                    drawCircle(colour.copy(alpha = 0.10f), radius = radius - stroke / 2, center = c)
                    if (halo) {
                        // Read here only: the halo's frames draw, nothing else's.
                        val l = level.floatValue
                        drawCircle(
                            colour.copy(alpha = 0.15f + 0.35f * l),
                            radius = radius + stroke * (0.6f + 2.2f * l),
                            center = c,
                            style = Stroke(width = stroke * (0.5f + 1.2f * l)),
                        )
                    }
                    val alpha = if (clocked && motion == RingMotion.BREATHE) {
                        0.45f + 0.55f * (0.5f + 0.5f * sin(2 * PI.toFloat() * clock.floatValue / BREATH_S))
                    } else {
                        1f
                    }
                    drawCircle(colour.copy(alpha = alpha), radius = radius, center = c, style = Stroke(width = stroke))
                    if (clocked && motion == RingMotion.SPIN) {
                        drawArc(
                            color = colour,
                            startAngle = (clock.floatValue / SPIN_S * 360f) % 360f,
                            sweepAngle = 70f,
                            useCenter = false,
                            topLeft = Offset(c.x - radius, c.y - radius),
                            size = arcBox,
                            style = Stroke(width = stroke * 1.8f, cap = StrokeCap.Round),
                        )
                    }
                    if (struck) {
                        val d = radius * 0.62f
                        drawLine(
                            colour,
                            start = Offset(c.x - d, c.y + d),
                            end = Offset(c.x + d, c.y - d),
                            strokeWidth = stroke * 0.8f,
                            cap = StrokeCap.Round,
                        )
                    }
                    if (badge != null) {
                        drawCircle(badge, radius = stroke * 1.3f, center = Offset(c.x + radius * 0.72f, c.y - radius * 0.72f))
                    }
                }
            },
    )
}

private const val ATTACK = 0.5f
private const val RELEASE = 0.12f

/** A level change below this is not drawn: half a hundredth of the halo's range, under a pixel. */
private const val LEVEL_EPSILON = 0.005f
private const val BREATH_S = 1.6f
private const val SPIN_S = 1.2f

/** The clock wraps at a common multiple of the periods, so no motion jumps. */
private const val CLOCK_WRAP_S = 48f
