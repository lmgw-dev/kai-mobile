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
import dev.rootshell.kai.ui.pairing.PairingScreen
import dev.rootshell.kai.ui.pairing.ScanScreen
import dev.rootshell.kai.ui.screens.MainScreen
import dev.rootshell.kai.ui.screens.SettingsScreen

/** Navigation Compose, one activity (design §10.2): the main destination, pairing, the scanner and the settings. */
object Routes {
    const val MAIN = "main"
    const val SETTINGS = "settings"
    const val PAIRING = "pairing"
    const val SCAN = "scan"
}

@Composable
fun KaiApp(app: KaiApplication, about: AboutState) {
    val nav = rememberNavController()
    val pendingRoute by app.hub.pendingRoute.collectAsStateWithLifecycle()
    LaunchedEffect(pendingRoute) {
        val route = pendingRoute ?: return@LaunchedEffect
        if (route == Routes.MAIN) nav.popBackStack(Routes.MAIN, inclusive = false) else nav.navigate(route)
        app.hub.pendingRoute.value = null
    }
    NavHost(navController = nav, startDestination = Routes.MAIN) {
        composable(Routes.MAIN) {
            MainScreen(
                app = app,
                onOpenSettings = { nav.navigate(Routes.SETTINGS) },
                onPair = { nav.navigate(Routes.PAIRING) },
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
