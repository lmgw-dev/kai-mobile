package dev.rootshell.kai.ui.screens

import android.annotation.SuppressLint
import android.app.Activity
import android.content.Context
import android.content.Intent
import android.os.Build
import android.provider.Settings as AndroidSettings
import android.security.keystore.KeyProperties
import androidx.activity.compose.BackHandler
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.ArrowBack
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.Button
import androidx.compose.material3.Checkbox
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.ListItem
import androidx.compose.material3.ListItemDefaults
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Switch
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.material3.TopAppBar
import androidx.compose.material3.VerticalDivider
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.produceState
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.unit.dp
import androidx.core.net.toUri
import androidx.lifecycle.compose.LifecycleResumeEffect
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import dev.rootshell.kai.AboutState
import dev.rootshell.kai.KaiApplication
import dev.rootshell.kai.R
import dev.rootshell.kai.app.BackgroundSetup
import dev.rootshell.kai.app.BackgroundStep
import dev.rootshell.kai.app.Notifier
import dev.rootshell.kai.core.AndroidKeystore
import dev.rootshell.kai.core.ffi.Colours
import dev.rootshell.kai.core.ffi.CoreException
import dev.rootshell.kai.ui.LabelledCheckbox
import dev.rootshell.kai.ui.LabelledSwitch
import dev.rootshell.kai.ui.Layout
import dev.rootshell.kai.ui.ScreenInsets
import dev.rootshell.kai.ui.SetupStep
import dev.rootshell.kai.ui.currentLayout
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext

/** The settings pages (design §10.1). */
enum class SettingsPage(val title: Int) {
    Gateway(R.string.settings_gateway),
    Connection(R.string.settings_connection),
    Background(R.string.settings_background),
    Assistant(R.string.settings_assistant),
    Notifications(R.string.settings_notifications),
    Voice(R.string.settings_voice),
    Appearance(R.string.settings_appearance),
    About(R.string.settings_about),
}

/**
 * Compact width: the list, then a page on its own screen. Wide: list and detail side by side
 * (design §10.6). The selected page survives a fold or an unfold (`rememberSaveable`).
 */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun SettingsScreen(app: KaiApplication, about: AboutState, onPair: () -> Unit, onBack: () -> Unit) {
    val layout = currentLayout()
    var selected by rememberSaveable { mutableStateOf<SettingsPage?>(null) }
    val page = if (layout == Layout.Wide) selected ?: SettingsPage.Gateway else selected

    // On a compact screen, back from a page returns to the list; on the list it leaves.
    BackHandler(enabled = layout == Layout.Compact && selected != null) { selected = null }

    Scaffold(
        // Edge to edge with the IME (design §10.2): a focused field scrolls above the keyboard.
        contentWindowInsets = ScreenInsets.content(),
        topBar = {
            TopAppBar(
                windowInsets = ScreenInsets.bar(),
                title = {
                    Text(stringResource(if (layout == Layout.Compact && page != null) page.title else R.string.settings_title))
                },
                navigationIcon = {
                    IconButton(onClick = { if (layout == Layout.Compact && selected != null) selected = null else onBack() }) {
                        Icon(Icons.AutoMirrored.Filled.ArrowBack, contentDescription = stringResource(R.string.back))
                    }
                },
            )
        },
    ) { padding ->
        val body = Modifier.fillMaxSize().padding(padding)
        when {
            layout == Layout.Wide -> Row(body) {
                SettingsList(Modifier.width(320.dp).fillMaxHeight(), page) { selected = it }
                VerticalDivider()
                Box(Modifier.weight(1f).fillMaxHeight()) { SettingsDetail(page, app, about, onPair) }
            }
            page == null -> SettingsList(body, null) { selected = it }
            else -> Box(body) { SettingsDetail(page, app, about, onPair) }
        }
    }
}

@Composable
private fun SettingsList(modifier: Modifier, selected: SettingsPage?, onSelect: (SettingsPage) -> Unit) {
    Column(modifier.verticalScroll(rememberScrollState())) {
        SettingsPage.entries.forEach { entry ->
            ListItem(
                headlineContent = { Text(stringResource(entry.title)) },
                modifier = Modifier.clickable { onSelect(entry) },
                colors = ListItemDefaults.colors(
                    containerColor = if (entry == selected) MaterialTheme.colorScheme.primaryContainer else MaterialTheme.colorScheme.surface,
                ),
            )
        }
    }
}

@Composable
private fun SettingsDetail(page: SettingsPage?, app: KaiApplication, about: AboutState, onPair: () -> Unit) {
    val scroll = Modifier.fillMaxSize().verticalScroll(rememberScrollState()).padding(vertical = 8.dp)
    when (page) {
        SettingsPage.Gateway -> GatewayPage(app, onPair, scroll)
        SettingsPage.Connection -> ConnectionPage(app, scroll)
        SettingsPage.Background -> BackgroundPage(scroll)
        SettingsPage.Assistant -> AssistantPage(scroll)
        SettingsPage.Notifications -> NotificationsPage(scroll)
        SettingsPage.Appearance -> AppearancePage(app, scroll)
        SettingsPage.About -> AboutPage(about, app.core.settingsNotice())
        SettingsPage.Voice -> VoicePage(app, scroll)
        null -> Unit
    }
}

/**
 * The stored pairing (design §10.1): its facts, the device key's security level (a warning when it
 * is neither the TEE nor StrongBox, design §4.3; the key is used all the same), Pair again, Forget
 * gateway.
 */
@Composable
private fun GatewayPage(app: KaiApplication, onPair: () -> Unit, modifier: Modifier) {
    val connection by app.hub.connection.collectAsStateWithLifecycle()
    val scope = rememberCoroutineScope()
    var confirmForget by remember { mutableStateOf(false) }
    val g = connection?.gateway
    val alias = g?.keyAlias
    val level by produceState<KeyLevel?>(null, alias) {
        value = alias?.let { a -> withContext(Dispatchers.IO) { KeyLevel(AndroidKeystore.securityLevel(a)) } }
    }
    Column(modifier) {
        if (g == null) {
            Fact(R.string.settings_gateway, stringResource(R.string.not_paired))
        } else {
            Fact(R.string.gateway_device, connection?.deviceName ?: g.name)
            g.urls.forEach { url ->
                Fact(R.string.gateway_url, if (url == g.lastGood) stringResource(R.string.gateway_url_last, url) else url)
            }
            Fact(R.string.gateway_fp, g.fpShort)
            Fact(R.string.gateway_cert, stringResource(R.string.gateway_cert_dates, g.notAfter, g.noticeAfter))
            connection?.certNotice?.let { Fact(R.string.gateway_notice, it) }
            Fact(R.string.gateway_code, g.confirmCode)
            g.dashboardUrl?.let { Fact(R.string.gateway_dashboard, it) }
            level?.let { l ->
                val name = stringResource(keyLevelName(l.level))
                if (AndroidKeystore.inSecureHardware(l.level)) {
                    Fact(R.string.gateway_key, name)
                } else {
                    Fact(R.string.gateway_key, stringResource(R.string.gateway_key_insecure, name), warning = true)
                }
            }
        }
        FlowRow(Modifier.padding(16.dp), horizontalArrangement = Arrangement.spacedBy(8.dp), verticalArrangement = Arrangement.spacedBy(4.dp)) {
            Button(onClick = onPair) { Text(stringResource(if (g == null) R.string.action_pair else R.string.action_pair_again)) }
            if (g != null) OutlinedButton(onClick = { confirmForget = true }) { Text(stringResource(R.string.forget_gateway)) }
        }
    }
    if (confirmForget) {
        AlertDialog(
            onDismissRequest = { confirmForget = false },
            title = { Text(stringResource(R.string.forget_gateway)) },
            text = { Text(stringResource(R.string.forget_gateway_text)) },
            confirmButton = {
                TextButton(onClick = {
                    confirmForget = false
                    scope.launch { app.core.forget() }
                }) { Text(stringResource(R.string.forget_gateway)) }
            },
            dismissButton = { TextButton(onClick = { confirmForget = false }) { Text(stringResource(R.string.cancel)) } },
        )
    }
}

/** The connection's settings (design §3.4): the waits, the stagger, the backoffs, the keep-alive. */
@Composable
private fun ConnectionPage(app: KaiApplication, modifier: Modifier) {
    val scope = rememberCoroutineScope()
    val start = remember { app.core.settings() }
    var timeout by remember { mutableStateOf(start.lmgw.timeoutS.toString()) }
    var stagger by remember { mutableStateOf(start.lmgw.urlStaggerMs.toString()) }
    var retry by remember { mutableStateOf(start.lmgw.retryMaxS.toString()) }
    var retryBackground by remember { mutableStateOf(start.lmgw.retryMaxBackgroundS.toString()) }
    var keepalive by remember { mutableStateOf(start.feed.keepaliveS.toString()) }
    var poll by remember { mutableStateOf(start.pairing.confirmPollS.toString()) }
    var folder by remember { mutableStateOf(start.conversation.folderName) }
    var said by remember { mutableStateOf<String?>(null) }
    Column(modifier.padding(horizontal = 16.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
        Number("lmgw.timeout_s", R.string.setting_timeout, timeout) { timeout = it }
        Number("lmgw.url_stagger_ms", R.string.setting_stagger, stagger) { stagger = it }
        Number("lmgw.retry_max_s", R.string.setting_retry, retry) { retry = it }
        Number("lmgw.retry_max_background_s", R.string.setting_retry_background, retryBackground) { retryBackground = it }
        Number("feed.keepalive_s", R.string.setting_keepalive, keepalive) { keepalive = it }
        Number("pairing.confirm_poll_s", R.string.setting_confirm_poll, poll) { poll = it }
        OutlinedTextField(
            value = folder,
            onValueChange = { folder = it },
            label = { Text("conversation.folder_name") },
            supportingText = { Text(stringResource(R.string.setting_folder)) },
            modifier = Modifier.fillMaxWidth(),
        )
        Text(stringResource(R.string.setting_doze_note), style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
        Button(onClick = {
            scope.launch {
                said = try {
                    val s = app.core.settings()
                    s.lmgw.timeoutS = timeout.trim().toUInt()
                    s.lmgw.urlStaggerMs = stagger.trim().toUInt()
                    s.lmgw.retryMaxS = retry.trim().toUInt()
                    s.lmgw.retryMaxBackgroundS = retryBackground.trim().toUInt()
                    s.feed.keepaliveS = keepalive.trim().toUInt()
                    s.pairing.confirmPollS = poll.trim().toUInt()
                    s.conversation.folderName = folder
                    app.core.setSettings(s)
                    app.getString(R.string.saved)
                } catch (e: NumberFormatException) {
                    app.getString(R.string.setting_not_a_number)
                } catch (e: CoreException) {
                    e.message
                }
            }
        }) { Text(stringResource(R.string.save)) }
        said?.let { Text(it, style = MaterialTheme.typography.bodyMedium) }
    }
}

@Composable
private fun Number(key: String, help: Int, value: String, onChange: (String) -> Unit) {
    OutlinedTextField(
        value = value,
        onValueChange = onChange,
        label = { Text(key) },
        supportingText = { Text(stringResource(help)) },
        keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Number),
        singleLine = true,
        modifier = Modifier.fillMaxWidth(),
    )
}

/** Setup's "Kai in the background" card (design §9.5), re-checked on resume. */
@Composable
fun BackgroundPage(modifier: Modifier = Modifier) {
    val context = LocalContext.current
    var facts by remember { mutableStateOf(BackgroundSetup.facts(context)) }
    LifecycleResumeEffect(Unit) {
        facts = BackgroundSetup.facts(context)
        onPauseOrDispose {}
    }
    val ask = rememberLauncherForActivityResult(ActivityResultContracts.RequestPermission()) {
        facts = BackgroundSetup.facts(context)
    }
    val open = BackgroundSetup.open(facts)
    Column(modifier.padding(horizontal = 16.dp), verticalArrangement = Arrangement.spacedBy(12.dp)) {
        Text(stringResource(R.string.background_intro), style = MaterialTheme.typography.bodyMedium)
        SetupStep(stringResource(R.string.background_notifications), facts.notificationsAllowed) {
            if (Build.VERSION.SDK_INT >= 33) {
                ask.launch(android.Manifest.permission.POST_NOTIFICATIONS)
            } else {
                context.startActivity(
                    Intent(AndroidSettings.ACTION_APP_NOTIFICATION_SETTINGS).putExtra(AndroidSettings.EXTRA_APP_PACKAGE, context.packageName),
                )
            }
        }
        SetupStep(stringResource(R.string.background_battery), facts.batteryExempt) { askBatteryExemption(context) }
        if (facts.samsung) {
            Text(stringResource(R.string.background_samsung), style = MaterialTheme.typography.bodyMedium)
            LabelledCheckbox(stringResource(R.string.background_samsung_done), facts.neverSleepingTicked) {
                BackgroundSetup.tickNeverSleeping(context, it)
                facts = BackgroundSetup.facts(context)
            }
            OutlinedButton(onClick = { openAppSettings(context) }) { Text(stringResource(R.string.background_app_settings)) }
        }
        Text(
            stringResource(if (open.isEmpty()) R.string.background_done else R.string.background_open),
            style = MaterialTheme.typography.labelLarge,
        )
        if (BackgroundStep.Notifications in open) {
            Text(stringResource(R.string.background_notifications_why), style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
        }
    }
}

private fun openAppSettings(context: Context) {
    context.startActivity(Intent(AndroidSettings.ACTION_APPLICATION_DETAILS_SETTINGS, "package:${context.packageName}".toUri()))
}

/**
 * The battery-optimisation exemption (design §9.5): the held feed is Kai's whole push channel
 * (rulings 6, 7). Play's policy on the request does not apply: Kai mobile is not on Play.
 */
@SuppressLint("BatteryLife")
private fun askBatteryExemption(context: Context) {
    context.startActivity(Intent(AndroidSettings.ACTION_REQUEST_IGNORE_BATTERY_OPTIMIZATIONS, "package:${context.packageName}".toUri()))
}

/** A link to each channel's system settings (design §10.1). */
@Composable
private fun NotificationsPage(modifier: Modifier) {
    val context = LocalContext.current
    Column(modifier) {
        listOf(
            Notifier.CHANNEL_APPROVALS to R.string.channel_approvals,
            Notifier.CHANNEL_MESSAGES to R.string.channel_messages,
            Notifier.CHANNEL_NOTICES to R.string.channel_notices,
            Notifier.CHANNEL_STATUS to R.string.channel_status,
        ).forEach { (channel, label) ->
            ListItem(
                headlineContent = { Text(stringResource(label)) },
                modifier = Modifier.clickable {
                    context.startActivity(
                        Intent(AndroidSettings.ACTION_CHANNEL_NOTIFICATION_SETTINGS)
                            .putExtra(AndroidSettings.EXTRA_APP_PACKAGE, context.packageName)
                            .putExtra(AndroidSettings.EXTRA_CHANNEL_ID, channel),
                    )
                },
            )
        }
    }
}

/** Kai's graphite and blue, or the system's dynamic colour (design Q6). */
@Composable
private fun AppearancePage(app: KaiApplication, modifier: Modifier) {
    val scope = rememberCoroutineScope()
    val context = LocalContext.current
    var system by remember { mutableStateOf(app.core.settings().appearance.colours == Colours.SYSTEM) }
    Column(modifier.padding(16.dp)) {
        LabelledSwitch(stringResource(R.string.appearance_system), system) { on ->
            system = on
            scope.launch {
                val s = app.core.settings()
                s.appearance.colours = if (on) Colours.SYSTEM else Colours.KAI
                runCatching { app.core.setSettings(s) }
                (context as? Activity)?.recreate()
            }
        }
    }
}

/** The app's version, the core's and the lmgw pin: what a bug report names (design §10.1). */
@Composable
fun AboutPage(about: AboutState, settingsNotice: String?) {
    Column(Modifier.fillMaxSize().verticalScroll(rememberScrollState()).padding(vertical = 8.dp)) {
        Fact(R.string.about_app_version, about.appVersion)
        Fact(R.string.about_core_version, about.coreVersion)
        Fact(R.string.about_core_build, about.coreBuild)
        Fact(R.string.about_lmgw_pin, about.lmgwPin)
        Fact(R.string.about_lmgw_tree, about.lmgwTree)
        if (settingsNotice != null) Fact(R.string.about_settings_notice, settingsNotice)
    }
}

@Composable
private fun Fact(label: Int, value: String, warning: Boolean = false) {
    ListItem(
        overlineContent = { Text(stringResource(label)) },
        headlineContent = { Text(value, color = if (warning) MaterialTheme.colorScheme.error else Color.Unspecified) },
    )
}

/** A device key's security level as read (null inside: it could not be read). */
private data class KeyLevel(val level: Int?)

private fun keyLevelName(level: Int?): Int = when (level) {
    KeyProperties.SECURITY_LEVEL_TRUSTED_ENVIRONMENT -> R.string.key_level_tee
    KeyProperties.SECURITY_LEVEL_STRONGBOX -> R.string.key_level_strongbox
    KeyProperties.SECURITY_LEVEL_SOFTWARE -> R.string.key_level_software
    KeyProperties.SECURITY_LEVEL_UNKNOWN_SECURE -> R.string.key_level_unknown_secure
    else -> R.string.key_level_unknown
}
