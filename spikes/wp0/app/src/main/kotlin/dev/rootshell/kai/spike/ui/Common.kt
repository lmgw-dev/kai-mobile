package dev.rootshell.kai.spike.ui

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ColumnScope
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.safeDrawingPadding
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.Card
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Switch
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import dev.rootshell.kai.spike.SpikeLog
import java.time.Instant
import java.time.ZoneId
import java.time.format.DateTimeFormatter

/** The WP0 points this app trials; the others wait for lmgw's calls route and feed. */
enum class Point(val key: String, val title: String, val question: String) {
    A("a", "(a) Invocation", "Does One UI 8 offer the spike as the side button's assistant, does the press reach the overlay, and how soon is its first frame (warm and cold, cover and inner)? Expand, setDisabledShowContext, the role dialog, LineageOS's power button."),
    B("b", "(b) The microphone type", "Can the specialUse service add the microphone type from the assistant session and from a visible activity, and drop it? Does the capture survive dismissal and 10 minutes of screen-off? The notification, and Wi-Fi latency with the screen off."),
    D("d", "(d) Echo cancelling", "Hardware against software echo cancelling on the speaker: false barge-ins at three volumes, folded and unfolded."),
    E("e", "(e) UniFFI, JNA and TLS", "APK size, the first call, an EventSink call from a tokio thread (JNA's attach and detach), 16 KB alignment, a Keystore signature through the callback cold and warm, a full TLS 1.3 handshake with client authentication."),
    I("i", "(i) Headset buttons", "Which presses reach the MediaSession, wired and Bluetooth; the SCO setup time; a capture over HFP and its sample rate."),
    N("n", "(n) The two screens", "The real dp and size classes of both screens; folding during a capture held by the service; the overlay on both screens; Continue apps on cover screen."),
    O("o", "(o) Self-check", "LineageOS without Google's services: the core, libwebrtc, a Keystore key that signs, the overlay and a notification; the Android version for M15."),
    ;

    companion object {
        fun of(key: String?) = entries.firstOrNull { it.key == key }
    }
}

/** A point's page: its question, what the tester does, its controls and results, its log. */
@Composable
fun PointPage(point: Point, onBack: () -> Unit, content: @Composable ColumnScope.() -> Unit) {
    Column(
        Modifier.fillMaxSize().safeDrawingPadding().verticalScroll(rememberScrollState()).padding(16.dp),
        verticalArrangement = Arrangement.spacedBy(12.dp),
    ) {
        Row(verticalAlignment = Alignment.CenterVertically) {
            TextButton(onClick = onBack) { Text("‹ Points") }
            Text(point.title, style = MaterialTheme.typography.titleLarge)
        }
        Text(point.question, style = MaterialTheme.typography.bodyMedium)
        Column(Modifier.widthIn(max = 840.dp), verticalArrangement = Arrangement.spacedBy(12.dp)) { content() }
        LogTail(point.key)
    }
}

/** What the person at the phone does, numbered. */
@Composable
fun Steps(vararg steps: String) {
    Card(Modifier.fillMaxWidth()) {
        Column(Modifier.padding(16.dp), verticalArrangement = Arrangement.spacedBy(6.dp)) {
            Text("What you do", style = MaterialTheme.typography.titleSmall)
            steps.forEachIndexed { i, s -> Text("${i + 1}. $s", style = MaterialTheme.typography.bodyMedium) }
        }
    }
}

@Composable
fun Results(title: String, lines: List<String>) {
    Card(Modifier.fillMaxWidth()) {
        Column(Modifier.padding(16.dp), verticalArrangement = Arrangement.spacedBy(4.dp)) {
            Text(title, style = MaterialTheme.typography.titleSmall)
            if (lines.isEmpty()) Text("nothing yet", style = MaterialTheme.typography.bodySmall)
            lines.forEach { Text(it, style = MaterialTheme.typography.bodySmall, fontFamily = FontFamily.Monospace) }
        }
    }
}

@Composable
fun Toggle(label: String, checked: Boolean, onChange: (Boolean) -> Unit) {
    Row(Modifier.fillMaxWidth(), verticalAlignment = Alignment.CenterVertically) {
        Text(label, Modifier.weight(1f), style = MaterialTheme.typography.bodyMedium)
        Switch(checked = checked, onCheckedChange = onChange)
    }
}

private val clock = DateTimeFormatter.ofPattern("HH:mm:ss").withZone(ZoneId.systemDefault())

/** The point's latest log lines; the file holds all of them. */
@Composable
fun LogTail(point: String) {
    val entries by SpikeLog.entries.collectAsState()
    val mine = entries.filter { it.point == point }.takeLast(40)
    Card(Modifier.fillMaxWidth()) {
        Column(Modifier.padding(16.dp), verticalArrangement = Arrangement.spacedBy(2.dp)) {
            Text("Log: the latest ${mine.size} lines of ($point); every line is in ${SpikeLog.path()}", style = MaterialTheme.typography.titleSmall)
            mine.asReversed().forEach {
                Text(
                    "${clock.format(Instant.ofEpochMilli(it.wallMs))} ${it.event} ${it.text}",
                    fontFamily = FontFamily.Monospace, fontSize = 11.sp, lineHeight = 14.sp,
                )
            }
        }
    }
}
