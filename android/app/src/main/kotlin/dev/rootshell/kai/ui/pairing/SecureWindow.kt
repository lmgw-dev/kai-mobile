package dev.rootshell.kai.ui.pairing

import android.view.Window
import android.view.WindowManager
import androidx.activity.compose.LocalActivity
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import java.util.WeakHashMap

/**
 * `FLAG_SECURE` on the activity's window while a pairing screen is shown (design §4.1): the link
 * and the scanner's frame carry a pairing token, which must reach neither a screenshot, a screen
 * recording nor the recents thumbnail. Counted per window, since the pairing screen and the scanner
 * overlap while the navigation moves between them.
 */
@Composable
fun SecureWhileShown() {
    val window = LocalActivity.current?.window ?: return
    DisposableEffect(window) {
        SecureWindows.acquire(window)
        onDispose { SecureWindows.release(window) }
    }
}

/** The count of pairing screens shown per window (the main thread's only). */
private object SecureWindows {
    private val counts = WeakHashMap<Window, Int>()

    fun acquire(window: Window) {
        val n = (counts[window] ?: 0) + 1
        counts[window] = n
        if (n == 1) window.addFlags(WindowManager.LayoutParams.FLAG_SECURE)
    }

    fun release(window: Window) {
        val n = (counts[window] ?: 1) - 1
        if (n <= 0) {
            counts.remove(window)
            window.clearFlags(WindowManager.LayoutParams.FLAG_SECURE)
        } else {
            counts[window] = n
        }
    }
}
