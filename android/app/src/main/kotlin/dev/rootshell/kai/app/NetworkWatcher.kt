package dev.rootshell.kai.app

import android.content.Context
import android.net.ConnectivityManager
import android.net.Network
import android.net.NetworkCapabilities
import dev.rootshell.kai.core.CoreFacade
import dev.rootshell.kai.core.ffi.NetworkFacts

/**
 * The default network as the core needs it (design §5.2): `registerDefaultNetworkCallback` hands
 * it whether one exists, which one, and its transport. Kotlin binds nothing; the core's sockets
 * follow Android's default routing, and the core races the URLs again on a change.
 */
class NetworkWatcher(private val context: Context, private val core: CoreFacade) {
    private var current: Network? = null

    fun start() {
        val cm = context.getSystemService(ConnectivityManager::class.java)
        cm.registerDefaultNetworkCallback(
            object : ConnectivityManager.NetworkCallback() {
                override fun onAvailable(network: Network) {
                    current = network
                    core.setNetwork(NetworkFacts(true, network.toString(), transport(cm.getNetworkCapabilities(network))))
                }

                override fun onCapabilitiesChanged(network: Network, caps: NetworkCapabilities) {
                    if (network == current) core.setNetwork(NetworkFacts(true, network.toString(), transport(caps)))
                }

                override fun onLost(network: Network) {
                    if (network == current) {
                        current = null
                        core.setNetwork(NetworkFacts(false, "", ""))
                    }
                }
            },
        )
    }

    private fun transport(caps: NetworkCapabilities?): String = when {
        caps == null -> ""
        caps.hasTransport(NetworkCapabilities.TRANSPORT_VPN) -> "vpn"
        caps.hasTransport(NetworkCapabilities.TRANSPORT_WIFI) -> "wifi"
        caps.hasTransport(NetworkCapabilities.TRANSPORT_CELLULAR) -> "cellular"
        caps.hasTransport(NetworkCapabilities.TRANSPORT_ETHERNET) -> "ethernet"
        else -> "other"
    }
}
