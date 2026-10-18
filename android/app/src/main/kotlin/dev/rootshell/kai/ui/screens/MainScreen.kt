package dev.rootshell.kai.ui.screens

import android.content.Context
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.consumeWindowInsets
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.widthIn
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Settings
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.ModalBottomSheet
import androidx.compose.material3.Scaffold
import androidx.compose.material3.SnackbarHost
import androidx.compose.material3.SnackbarHostState
import androidx.compose.material3.Text
import androidx.compose.material3.TopAppBar
import androidx.compose.material3.VerticalDivider
import androidx.compose.material3.rememberModalBottomSheetState
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import dev.rootshell.kai.KaiApplication
import dev.rootshell.kai.R
import dev.rootshell.kai.core.ffi.ConnectionAction
import dev.rootshell.kai.ui.Layout
import dev.rootshell.kai.ui.ScreenInsets
import dev.rootshell.kai.ui.conversation.ConversationPane
import dev.rootshell.kai.ui.currentLayout
import dev.rootshell.kai.ui.voice.VoicePane

/**
 * The start destination (design §10.1, §10.6). Compact width (a phone, or a foldable's cover screen): the voice
 * screen with the link's state, the conversation one tap away as a sheet. Wide, its inner screen:
 * voice and conversation side by side. The voice pane ([VoicePane]) shows the connection (§5.3)
 * with its words and actions, and once paired the call: the ring, its captions and controls.
 */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun MainScreen(app: KaiApplication, onOpenSettings: () -> Unit, onPair: () -> Unit) {
    val hub = app.hub
    val core = app.core
    val layout = currentLayout()
    val connection by hub.connection.collectAsStateWithLifecycle()
    var sheetOpen by rememberSaveable { mutableStateOf(false) }
    val snackbar = remember { SnackbarHostState() }
    val context = LocalContext.current
    LaunchedEffect(hub) { hub.notices.collect { snackbar.showSnackbar(it) } }
    val openConversation by hub.openConversation.collectAsStateWithLifecycle()
    LaunchedEffect(openConversation) {
        if (openConversation) {
            sheetOpen = true
            hub.openConversation.value = false
        }
    }
    val onAction: (ConnectionAction) -> Unit = { action ->
        when (action) {
            ConnectionAction.PAIR, ConnectionAction.PAIR_AGAIN -> onPair()
            ConnectionAction.CHECK_NOW, ConnectionAction.TRY_AGAIN -> core.retry()
            ConnectionAction.OPEN_WIRE_GUARD -> openWireGuard(context)
        }
    }
    Scaffold(
        // Edge to edge (design §10.2): the system bars and the display cutout (the cover screen's
        // camera in landscape) pad every screen, the IME too, so the conversation's field rises
        // with the keyboard on the inner screen.
        contentWindowInsets = ScreenInsets.content(),
        topBar = {
            TopAppBar(
                windowInsets = ScreenInsets.bar(),
                title = { Text(stringResource(R.string.app_name)) },
                actions = {
                    IconButton(onClick = onOpenSettings) {
                        Icon(Icons.Default.Settings, contentDescription = stringResource(R.string.open_settings))
                    }
                },
            )
        },
        snackbarHost = { SnackbarHost(snackbar) },
    ) { padding ->
        // Consumed, so the conversation's own IME padding counts only what the Scaffold left.
        Box(Modifier.fillMaxSize().padding(padding).consumeWindowInsets(padding)) {
            when (layout) {
                Layout.Compact -> VoicePane(
                    app = app,
                    connection = connection,
                    onAction = onAction,
                    modifier = Modifier.fillMaxSize(),
                    onOpenConversation = { sheetOpen = true },
                    onSetup = onOpenSettings,
                )
                Layout.Wide -> Row(Modifier.fillMaxSize()) {
                    VoicePane(app, connection, onAction, Modifier.weight(1f).fillMaxHeight(), onOpenConversation = null, onSetup = onOpenSettings)
                    VerticalDivider()
                    // The conversation keeps a readable measure instead of running the full width.
                    Box(Modifier.weight(1f).fillMaxHeight(), contentAlignment = Alignment.TopCenter) {
                        ConversationPane(app, Modifier.widthIn(max = 640.dp).fillMaxSize())
                    }
                }
            }
        }
    }
    if (layout == Layout.Compact && sheetOpen) {
        ModalBottomSheet(
            onDismissRequest = { sheetOpen = false },
            sheetState = rememberModalBottomSheetState(skipPartiallyExpanded = true),
        ) {
            ConversationPane(app, Modifier.fillMaxWidth().fillMaxHeight(0.92f))
        }
    }
}

/** Launches the WireGuard app (design §5.3: the case away from home with the tunnel down). */
private fun openWireGuard(context: Context) {
    context.packageManager.getLaunchIntentForPackage("com.wireguard.android")?.let { context.startActivity(it) }
}

fun actionLabel(action: ConnectionAction): Int = when (action) {
    ConnectionAction.PAIR -> R.string.action_pair
    ConnectionAction.PAIR_AGAIN -> R.string.action_pair_again
    ConnectionAction.CHECK_NOW -> R.string.action_check_now
    ConnectionAction.TRY_AGAIN -> R.string.action_try_again
    ConnectionAction.OPEN_WIRE_GUARD -> R.string.action_open_wireguard
}
