package dev.rootshell.kai.ui.pairing

import android.content.ClipboardManager
import android.content.Context
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.ArrowBack
import androidx.compose.material3.Button
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Text
import androidx.compose.material3.TopAppBar
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import dev.rootshell.kai.R
import dev.rootshell.kai.app.CoreHub
import dev.rootshell.kai.core.CoreFacade
import dev.rootshell.kai.core.ffi.CoreException
import dev.rootshell.kai.core.ffi.PairingPreview
import dev.rootshell.kai.ui.ScreenInsets
import kotlinx.coroutines.launch

/**
 * Pairing (design §4.1, §4.2): the link v2 by paste, by a tap (an `lmgw-pair:` link hands it over)
 * or by a scan, always leading to this confirmation first (kai K13: the facts, the URLs, what it
 * replaces). Pair enrols the Keystore key with exactly the text the shown facts came from (never a
 * text changed since), and starts the confirmation's wait. The link's text lives in memory only and
 * is dropped after a pairing; trying the same link again sends the same key. The field takes no
 * autocorrect (Compose offers no switch for the keyboard's personalised learning; the URI keyboard
 * type is the closest), and the window is `FLAG_SECURE` while the screen shows.
 */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun PairingScreen(hub: CoreHub, core: CoreFacade, onScan: () -> Unit, onDone: () -> Unit, onBack: () -> Unit) {
    SecureWhileShown()
    val text by hub.pairingText.collectAsStateWithLifecycle()
    var preview by remember { mutableStateOf<Previewed?>(null) }
    var problem by remember { mutableStateOf<String?>(null) }
    var pairing by remember { mutableStateOf(false) }
    val scope = rememberCoroutineScope()
    val context = LocalContext.current
    LaunchedEffect(text) {
        preview = null
        problem = null
        if (text.isNotBlank()) {
            try {
                preview = Previewed(text, core.pairingPreview(text))
            } catch (e: CoreException) {
                problem = e.message
            }
        }
    }
    Scaffold(
        // Edge to edge with the IME (design §10.2): the field and Pair stay above the keyboard.
        contentWindowInsets = ScreenInsets.content(),
        topBar = {
            TopAppBar(
                windowInsets = ScreenInsets.bar(),
                title = { Text(stringResource(R.string.pairing_title)) },
                navigationIcon = {
                    IconButton(onClick = onBack) {
                        Icon(Icons.AutoMirrored.Filled.ArrowBack, contentDescription = stringResource(R.string.back))
                    }
                },
            )
        },
    ) { padding ->
        // A readable measure on a wide window (the inner screen, design §10.6), centred.
        Box(Modifier.fillMaxSize().padding(padding), contentAlignment = Alignment.TopCenter) {
        Column(
            Modifier.widthIn(max = 640.dp).fillMaxSize().verticalScroll(rememberScrollState()).padding(24.dp),
            verticalArrangement = Arrangement.spacedBy(12.dp),
        ) {
            Text(stringResource(R.string.pairing_intro), style = MaterialTheme.typography.bodyLarge)
            OutlinedTextField(
                value = text,
                onValueChange = { hub.pairingText.value = it },
                label = { Text(stringResource(R.string.pairing_link)) },
                modifier = Modifier.fillMaxWidth(),
                keyboardOptions = KeyboardOptions(autoCorrectEnabled = false, keyboardType = KeyboardType.Uri),
                minLines = 2,
                maxLines = 5,
            )
            FlowRow(horizontalArrangement = Arrangement.spacedBy(8.dp), verticalArrangement = Arrangement.spacedBy(4.dp)) {
                OutlinedButton(onClick = { paste(context)?.let { hub.pairingText.value = it } }) {
                    Text(stringResource(R.string.pairing_paste))
                }
                OutlinedButton(onClick = onScan) { Text(stringResource(R.string.pairing_scan)) }
            }
            Text(stringResource(R.string.pairing_scan_note), style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
            preview?.let { (shownText, p) ->
                p.lines.forEach { line ->
                    Text(
                        line.text,
                        style = MaterialTheme.typography.bodyMedium,
                        color = if (line.warning) MaterialTheme.colorScheme.error else MaterialTheme.colorScheme.onSurface,
                    )
                }
                Button(
                    enabled = !pairing,
                    onClick = {
                        pairing = true
                        problem = null
                        scope.launch {
                            try {
                                core.pair(shownText)
                                hub.pairingText.value = ""
                                onDone()
                            } catch (e: CoreException) {
                                problem = e.message
                            } finally {
                                pairing = false
                            }
                        }
                    },
                ) { Text(stringResource(if (pairing) R.string.pairing_in_progress else R.string.action_pair)) }
            }
            problem?.let { Text(it, color = MaterialTheme.colorScheme.error, style = MaterialTheme.typography.bodyMedium) }
        }
        }
    }
}

/** The facts shown and the text they came from: Pair sends that text. */
private data class Previewed(val text: String, val preview: PairingPreview)

/** The clipboard's text (a paste; surrounding whitespace is the parser's to ignore). */
private fun paste(context: Context): String? {
    val clip = context.getSystemService(ClipboardManager::class.java)?.primaryClip ?: return null
    if (clip.itemCount == 0) return null
    return clip.getItemAt(0).coerceToText(context)?.toString()
}
