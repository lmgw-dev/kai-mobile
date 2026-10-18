package dev.rootshell.kai.ui

import androidx.compose.foundation.layout.WindowInsets
import androidx.compose.foundation.layout.WindowInsetsSides
import androidx.compose.foundation.layout.only
import androidx.compose.foundation.layout.safeDrawing
import androidx.compose.material3.adaptive.currentWindowAdaptiveInfoV2
import androidx.compose.runtime.Composable
import androidx.window.core.layout.WindowSizeClass
import kotlin.math.max

/**
 * The layouts of design §10.6, by width class and never by device: compact is a foldable's cover
 * screen (and any phone), wide is its inner screen, a tablet, or a split window of 600 dp or more.
 */
enum class Layout { Compact, Wide }

fun layoutForWidthDp(widthDp: Int): Layout =
    if (widthDp >= WindowSizeClass.WIDTH_DP_MEDIUM_LOWER_BOUND) Layout.Wide else Layout.Compact

@Composable
fun currentLayout(): Layout = layoutForWidthDp(currentWindowAdaptiveInfoV2().windowSizeClass.minWidthDp)

/**
 * The insets every screen pads by (design §10.2, edge to edge): `safeDrawing`, the system bars, the
 * display cutout (a foldable's cover-screen camera in landscape, the inner screen's under-display
 * camera) and the IME. A Scaffold's content takes them whole, its top bar the top and the sides.
 */
object ScreenInsets {
    @Composable
    fun content(): WindowInsets = WindowInsets.safeDrawing

    @Composable
    fun bar(): WindowInsets = WindowInsets.safeDrawing.only(WindowInsetsSides.Horizontal + WindowInsetsSides.Top)
}

/** How the voice pane arranges itself in its window (design §10.6, WP3 part 2; WP5's font scale). */
enum class VoiceArrangement {
    /** The ring above, the captions filling the middle, the controls at the bottom. */
    TALL,

    /** Short and wider than tall (the cover screen in landscape): the ring beside the rest, which scrolls. */
    SIDE,

    /** Short and narrow (a split screen, a large font): one column that scrolls, so the controls stay reachable. */
    SCROLL,
}

/** The height the tall arrangement needs at a font scale of 1, without an approval card. */
const val VOICE_TALL_DP = 440f

/** What an approval card adds to it at a font scale of 1. */
const val VOICE_CARD_DP = 220f

/**
 * The voice pane's arrangement: tall while the window holds the ring, the words, a card and the
 * controls at this font scale (Android's up to 200 %, which grows the text and not the ring), else
 * the ring beside the rest when wider than tall, else one scrolling column.
 */
fun voiceArrangement(heightDp: Float, widthDp: Float, fontScale: Float, card: Boolean): VoiceArrangement {
    val scale = max(1f, fontScale)
    val needed = VOICE_TALL_DP * scale + if (card) VOICE_CARD_DP * scale else 0f
    return when {
        heightDp >= needed -> VoiceArrangement.TALL
        widthDp > heightDp -> VoiceArrangement.SIDE
        else -> VoiceArrangement.SCROLL
    }
}
