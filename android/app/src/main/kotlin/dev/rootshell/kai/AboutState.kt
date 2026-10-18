package dev.rootshell.kai

import dev.rootshell.kai.core.CoreFacade

/** What the About page shows (design §10.1): the app's version, the core's and the lmgw pin. */
data class AboutState(
    val appVersion: String,
    val coreVersion: String,
    val coreBuild: String,
    val lmgwPin: String,
    val lmgwTree: String,
) {
    companion object {
        fun from(appVersion: String, core: CoreFacade): AboutState {
            val pin = core.lmgwPin()
            val build = core.build()
            return AboutState(
                appVersion = appVersion,
                coreVersion = core.version(),
                coreBuild = "${build.profile}, ${build.target}",
                lmgwPin = "${pin.commit.take(7)} (${pin.tag})",
                lmgwTree = pin.tree.take(12),
            )
        }
    }
}
