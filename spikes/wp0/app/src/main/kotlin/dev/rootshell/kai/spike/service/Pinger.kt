package dev.rootshell.kai.spike.service

import android.content.Context
import android.net.ConnectivityManager
import android.net.NetworkCapabilities
import android.net.wifi.WifiInfo
import android.os.PowerManager
import android.os.SystemClock
import dev.rootshell.kai.spike.SpikeLog
import dev.rootshell.kai.spike.r2
import java.net.InetSocketAddress
import java.net.Socket

/**
 * (b)'s latency loop, once a second, held by the service so it runs with the screen off: ICMP
 * through the system's `ping` for a host, a TCP connect for `host:port`. Each sample keeps
 * whether the screen was on, so screen-on and screen-off latencies come apart (design §8.6: no
 * Wi-Fi lock; does Wi-Fi power saving cost latency?).
 */
class Pinger(private val context: Context, private val target: String) {
    data class Sample(val elapsedMs: Long, val rttMs: Double?, val screenOn: Boolean)

    data class Stats(
        val target: String,
        val mode: String,
        val sent: Int,
        val lost: Int,
        val screenOn: Summary,
        val screenOff: Summary,
        val last: Double?,
        val error: String?,
    ) {
        fun toMap(): Map<String, Any?> = mapOf(
            "target" to target, "mode" to mode, "sent" to sent, "lost" to lost,
            "screen_on" to screenOn.toMap(), "screen_off" to screenOff.toMap(), "error" to error,
        )
    }

    data class Summary(val count: Int, val median: Double?, val p95: Double?, val max: Double?) {
        fun toMap(): Map<String, Any?> = mapOf("count" to count, "median_ms" to median, "p95_ms" to p95, "max_ms" to max)
        override fun toString() = if (count == 0) "none" else "n=$count median ${median}ms p95 ${p95}ms max ${max}ms"

        companion object {
            fun of(rtts: List<Double>): Summary {
                if (rtts.isEmpty()) return Summary(0, null, null, null)
                val s = rtts.sorted()
                fun rank(q: Double) = s[(Math.ceil(q * s.size).toInt().coerceIn(1, s.size)) - 1].r2()
                return Summary(s.size, rank(0.5), rank(0.95), s.last().r2())
            }
        }
    }

    private val samples = mutableListOf<Sample>()
    @Volatile private var running = false
    @Volatile private var error: String? = null
    private var process: Process? = null
    private var thread: Thread? = null
    private val power = context.getSystemService(PowerManager::class.java)
    private val tcp = target.substringAfterLast(':', "").toIntOrNull()

    fun start() {
        running = true
        thread = Thread({ if (tcp != null) tcpLoop(target.substringBeforeLast(':'), tcp) else icmpLoop() }, "spike-ping")
            .also { it.start() }
    }

    fun stop(): Stats {
        running = false
        process?.destroy()
        thread?.join(3000)
        return stats()
    }

    fun stats(): Stats = synchronized(samples) {
        val ok = samples.filter { it.rttMs != null }
        Stats(
            target, if (tcp != null) "tcp" else "icmp", samples.size, samples.count { it.rttMs == null },
            Summary.of(ok.filter { it.screenOn }.map { it.rttMs!! }),
            Summary.of(ok.filter { !it.screenOn }.map { it.rttMs!! }),
            samples.lastOrNull()?.rttMs, error,
        )
    }

    private fun add(rtt: Double?) {
        val s = Sample(SystemClock.elapsedRealtime(), rtt, power.isInteractive)
        synchronized(samples) { samples += s }
        // Every sample in the log, with the Wi-Fi link when there is one.
        SpikeLog.event("b", "ping", mapOf("rtt_ms" to rtt?.r2(), "screen_on" to s.screenOn) + wifi())
    }

    private fun wifi(): Map<String, Any?> = runCatching {
        val cm = context.getSystemService(ConnectivityManager::class.java)
        val caps = cm.getNetworkCapabilities(cm.activeNetwork) ?: return mapOf("network" to "none")
        val info = caps.transportInfo as? WifiInfo
        when {
            info != null -> mapOf("network" to "wifi", "rssi" to info.rssi, "link_mbps" to info.linkSpeed, "freq_mhz" to info.frequency)
            caps.hasTransport(NetworkCapabilities.TRANSPORT_CELLULAR) -> mapOf("network" to "cellular")
            caps.hasTransport(NetworkCapabilities.TRANSPORT_VPN) -> mapOf("network" to "vpn")
            else -> mapOf("network" to "other")
        }
    }.getOrElse { mapOf("network" to "unreadable") }

    private fun tcpLoop(host: String, port: Int) {
        while (running) {
            val t = System.nanoTime()
            val rtt = try {
                Socket().use { it.connect(InetSocketAddress(host, port), 2000) }
                (System.nanoTime() - t) / 1e6
            } catch (e: Exception) {
                error = "${e.javaClass.simpleName}: ${e.message}"
                null
            }
            add(rtt)
            val rest = 1000 - (System.nanoTime() - t) / 1_000_000
            if (rest > 0) Thread.sleep(rest)
        }
    }

    private val reply = Regex("""icmp_seq=(\d+).*time=([\d.]+) ms""")

    private fun icmpLoop() {
        val p = try {
            ProcessBuilder("ping", "-n", "-i", "1", "-W", "2", target).redirectErrorStream(true).start()
        } catch (e: Exception) {
            error = "${e.javaClass.simpleName}: ${e.message}"
            return
        }
        process = p
        var lastSeq = 0
        p.inputStream.bufferedReader().useLines { lines ->
            for (line in lines) {
                if (!running) break
                val m = reply.find(line)
                if (m == null) {
                    if (line.contains("unknown host", true) || line.contains("unreachable", true)) error = line.trim()
                    continue
                }
                val seq = m.groupValues[1].toInt()
                // Sequence numbers that never answered are lost packets.
                for (missing in lastSeq + 1 until seq) add(null)
                lastSeq = seq
                add(m.groupValues[2].toDouble())
            }
        }
    }
}
