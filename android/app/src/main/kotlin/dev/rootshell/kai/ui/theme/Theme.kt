package dev.rootshell.kai.ui.theme

import androidx.compose.foundation.isSystemInDarkTheme
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.darkColorScheme
import androidx.compose.material3.dynamicDarkColorScheme
import androidx.compose.material3.dynamicLightColorScheme
import androidx.compose.material3.lightColorScheme
import androidx.compose.runtime.Composable
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.LocalContext
import dev.rootshell.kai.core.Colours

/** Kai's graphite and blue (design Q6): Breeze-like, never pastel. */
private val Graphite = Color(0xFF31363B)
private val GraphiteDeep = Color(0xFF1D2125)
private val Blue = Color(0xFF3DAEE9)
private val BlueDeep = Color(0xFF1B6EA8)

private val KaiDark = darkColorScheme(
    primary = Blue,
    onPrimary = GraphiteDeep,
    primaryContainer = Color(0xFF1F4F6E),
    onPrimaryContainer = Color(0xFFD6ECFA),
    secondary = Color(0xFFB4BBC2),
    background = GraphiteDeep,
    onBackground = Color(0xFFEFF0F1),
    surface = GraphiteDeep,
    onSurface = Color(0xFFEFF0F1),
    surfaceVariant = Graphite,
    onSurfaceVariant = Color(0xFFB4BBC2),
    surfaceContainer = Color(0xFF262B30),
    surfaceContainerHigh = Graphite,
    outline = Color(0xFF6B737A),
)

private val KaiLight = lightColorScheme(
    primary = BlueDeep,
    onPrimary = Color.White,
    primaryContainer = Color(0xFFD6ECFA),
    onPrimaryContainer = Color(0xFF0B3957),
    secondary = Color(0xFF4D545B),
    background = Color(0xFFEFF0F1),
    onBackground = GraphiteDeep,
    surface = Color(0xFFEFF0F1),
    onSurface = GraphiteDeep,
    surfaceVariant = Color(0xFFDDE0E3),
    onSurfaceVariant = Color(0xFF4D545B),
    surfaceContainer = Color(0xFFE5E7E9),
    surfaceContainerHigh = Color(0xFFDDE0E3),
    outline = Color(0xFF8A9199),
)

/**
 * Material 3 with Material's own typography and shapes (the system font, design §10.2); the colours
 * are Kai's graphite and blue, or the system's dynamic colour when the owner switches it on
 * (`appearance.colours`).
 */
@Composable
fun KaiTheme(colours: Colours = Colours.Kai, content: @Composable () -> Unit) {
    val dark = isSystemInDarkTheme()
    val scheme = when {
        colours == Colours.System -> { // dynamic colour exists from API 31, our minSdk
            val context = LocalContext.current
            if (dark) dynamicDarkColorScheme(context) else dynamicLightColorScheme(context)
        }
        dark -> KaiDark
        else -> KaiLight
    }
    MaterialTheme(colorScheme = scheme, content = content)
}
