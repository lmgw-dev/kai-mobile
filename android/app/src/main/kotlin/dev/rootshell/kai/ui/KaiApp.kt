package dev.rootshell.kai.ui

import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.navigation.compose.NavHost
import androidx.navigation.compose.composable
import androidx.navigation.compose.rememberNavController
import dev.rootshell.kai.AboutState
import dev.rootshell.kai.KaiApplication
import dev.rootshell.kai.app.ThreadTarget
import dev.rootshell.kai.ui.pairing.PairingScreen
import dev.rootshell.kai.ui.pairing.ScanScreen
import dev.rootshell.kai.ui.screens.MainScreen
import dev.rootshell.kai.ui.screens.SettingsScreen
import dev.rootshell.kai.ui.threads.ThreadsScreen

/**
 * Navigation Compose, one activity (design §10.2): the main destination, pairing, the scanner, the
 * settings and the threads browser (§6.4).
 */
object Routes {
    const val MAIN = "main"
    const val SETTINGS = "settings"
    const val PAIRING = "pairing"
    const val SCAN = "scan"
    const val THREADS = "threads"
}

@Composable
fun KaiApp(app: KaiApplication, about: AboutState) {
    val nav = rememberNavController()
    val pendingRoute by app.hub.pendingRoute.collectAsStateWithLifecycle()
    LaunchedEffect(pendingRoute) {
        val route = pendingRoute ?: return@LaunchedEffect
        when (route) {
            Routes.MAIN -> nav.popBackStack(Routes.MAIN, inclusive = false)
            // A thread a call is bound to (the overlay's Keyboard, design §7.12): the browser once.
            Routes.THREADS -> nav.navigate(route) { launchSingleTop = true }
            else -> nav.navigate(route)
        }
        app.hub.pendingRoute.value = null
    }
    // The browser's lists are read while it is the destination shown, and leaving it closes the
    // opened thread (design §6.4); a fold's recreation reports the same destination again.
    LaunchedEffect(nav) {
        nav.currentBackStackEntryFlow.collect { app.hub.showBrowser(it.destination.route == Routes.THREADS) }
    }
    NavHost(navController = nav, startDestination = Routes.MAIN) {
        composable(Routes.MAIN) {
            MainScreen(
                app = app,
                onOpenSettings = { nav.navigate(Routes.SETTINGS) },
                onPair = { nav.navigate(Routes.PAIRING) },
                onOpenThreads = { nav.navigate(Routes.THREADS) { launchSingleTop = true } },
                onOpenThread = { id ->
                    // The thread a call is bound to (design §7.12): opened in the browser.
                    app.hub.select(ThreadTarget(id))
                    nav.navigate(Routes.THREADS) { launchSingleTop = true }
                },
            )
        }
        composable(Routes.THREADS) {
            ThreadsScreen(
                app = app,
                onBack = { nav.popBackStack() },
                onOpenKai = {
                    app.hub.openConversation.value = true
                    nav.popBackStack(Routes.MAIN, inclusive = false)
                },
                // The voice screen (design §7.12: the bound thread's Show call).
                onShowCall = { nav.popBackStack(Routes.MAIN, inclusive = false) },
            )
        }
        composable(Routes.PAIRING) {
            PairingScreen(
                hub = app.hub,
                core = app.core,
                onScan = { nav.navigate(Routes.SCAN) },
                onDone = { nav.popBackStack(Routes.MAIN, inclusive = false) },
                onBack = { nav.popBackStack() },
            )
        }
        composable(Routes.SCAN) {
            ScanScreen(
                onResult = { text ->
                    app.hub.pairingText.value = text
                    nav.popBackStack()
                },
                onBack = { nav.popBackStack() },
            )
        }
        composable(Routes.SETTINGS) {
            SettingsScreen(
                app = app,
                about = about,
                onPair = { nav.navigate(Routes.PAIRING) },
                onBack = { nav.popBackStack() },
            )
        }
    }
}
