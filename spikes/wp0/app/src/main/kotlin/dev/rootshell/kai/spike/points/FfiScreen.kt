package dev.rootshell.kai.spike.points

import android.content.Intent
import android.os.Bundle
import android.os.Handler
import android.os.Looper
import android.os.ResultReceiver
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.ExperimentalLayoutApi
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.material3.Button
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.unit.dp
import dev.rootshell.kai.spike.SpikeLog
import dev.rootshell.kai.spike.probe.FfiBench
import dev.rootshell.kai.spike.probe.FfiProbeService
import dev.rootshell.kai.spike.ui.Point
import dev.rootshell.kai.spike.ui.PointPage
import dev.rootshell.kai.spike.ui.Results
import dev.rootshell.kai.spike.ui.Steps
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import org.json.JSONObject

/** (e)'s result as lines for the screen. */
internal fun ffiLines(j: JSONObject): List<String> {
    if (!j.optBoolean("ok")) return listOf("failed: ${j.optString("error")}")
    fun t(o: JSONObject?, unit: Double = 1.0, suffix: String = "µs") = if (o == null) "?" else
        "first ${fmt(o.optDouble("first_us") / unit)} median ${fmt(o.optDouble("median_us") / unit)} p90 ${fmt(o.optDouble("p90_us") / unit)} max ${fmt(o.optDouble("max_us") / unit)} $suffix (n=${o.optInt("count")})"
    val out = mutableListOf<String>()
    out += "${j.optString("where")}: process age ${j.optLong("process_age_ms")} ms, loaded before ${j.optBoolean("loaded_before")}, keep attached ${j.optBoolean("keep_attached")}"
    out += "APK ${"%.1f".format(j.optLong("apk_bytes") / 1048576.0)} MiB, page size ${j.optLong("page_size")}, all libraries 16 KB aligned: ${j.optBoolean("libs_all_16k")}"
    j.optJSONArray("libs")?.let { a ->
        for (i in 0 until a.length()) {
            val l = a.getJSONObject(i)
            out += "  ${l.optString("name")}: ${"%.1f".format(l.optLong("size") / 1048576.0)} MiB, stored ${l.optBoolean("stored")}, zip 16K ${l.optBoolean("zip_16k")}, p_align 0x${java.lang.Long.toHexString(l.optLong("min_load_align"))}"
        }
    }
    out += "first call: JNA ${j.optDouble("jna_init_ms")} ms, core ${j.optDouble("core_first_call_ms")} ms (load + Core::new), then ${j.optDouble("core_second_call_us")} µs, mean ${j.optDouble("core_call_mean_us")} µs; spike ${j.optDouble("spike_first_call_ms")} ms"
    j.optJSONObject("sink")?.let { s ->
        out += "EventSink, calling thread: ${t(s.optJSONObject("calling_thread"))}"
        out += "EventSink, tokio worker first: ${t(s.optJSONObject("worker_first_call"))}"
        out += "EventSink, tokio worker again: ${t(s.optJSONObject("worker_repeat_call"))}"
        out += "EventSink, fresh thread: ${t(s.optJSONObject("fresh_thread"))}"
        out += "  attach + detach per call ≈ ${s.optDouble("attach_detach_us")} µs (${s.optInt("workers_used")} workers)"
    }
    j.optJSONObject("keystore")?.let { k ->
        out += "Keystore (${k.optString("security_level")}): generate ${k.optDouble("generate_ms")} ms; sign ${t(k.optJSONObject("sign"), 1000.0, "ms")}; verified ${k.optInt("verified")}"
    }
    j.optJSONObject("keystore_direct")?.let { d ->
        out += "  straight from Kotlin: first ${d.optDouble("first_us") / 1000} ms, warm median ${d.optDouble("warm_median_us") / 1000} ms"
    }
    j.optJSONObject("tls")?.let { x ->
        out += "TLS: CA ${x.optDouble("ca_ms")} ms, leaf ${x.optDouble("server_leaf_ms")} ms, device cert ${x.optDouble("device_cert_ms")} ms"
        out += "  handshake ${t(x.optJSONObject("handshake"), 1000.0, "ms")}"
        out += "  its signature ${t(x.optJSONObject("sign"), 1000.0, "ms")}"
        out += "  ${x.optString("protocol")} ${x.optString("cipher_suite")} ${x.optString("key_exchange")}, client certificate verified ${x.optBoolean("client_cert_verified")}"
    }
    return out
}

private fun fmt(v: Double) = "%.2f".format(v)

@OptIn(ExperimentalLayoutApi::class)
@Composable
fun FfiScreen(onBack: () -> Unit) {
    val context = LocalContext.current
    val scope = rememberCoroutineScope()
    var lines by remember { mutableStateOf(emptyList<String>()) }
    var running by remember { mutableStateOf(false) }
    fun fresh(keep: Boolean) {
        running = true
        lines = listOf("running in a fresh process…")
        val receiver = object : ResultReceiver(Handler(Looper.getMainLooper())) {
            override fun onReceiveResult(code: Int, data: Bundle?) {
                val json = JSONObject(data?.getString(FfiProbeService.EXTRA_JSON) ?: "{}")
                SpikeLog.event("e", "result", mapOf("result" to json))
                lines = ffiLines(json)
                running = false
            }
        }
        context.startService(
            Intent(context, FfiProbeService::class.java)
                .putExtra(FfiProbeService.EXTRA_RECEIVER, receiver)
                .putExtra(FfiProbeService.EXTRA_KEEP_ATTACHED, keep),
        )
    }
    PointPage(Point.E, onBack) {
        Steps(
            "Run 5 times: each run starts a fresh process, so the first call and the first signature are cold (about 5 s each).",
            "Run once with JNA keeping tokio's threads attached: the EventSink lines show what the attach and detach cost per call.",
            "Run here measures in this process (cold only the first time after the app starts).",
        )
        FlowRow(horizontalArrangement = Arrangement.spacedBy(8.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
            Button(enabled = !running, onClick = { fresh(false) }) { Text("Run (fresh process)") }
            OutlinedButton(enabled = !running, onClick = { fresh(true) }) { Text("Run, threads kept attached") }
            OutlinedButton(enabled = !running, onClick = {
                running = true
                scope.launch {
                    val json = withContext(Dispatchers.IO) { FfiBench.run(context.applicationContext, false, "this_process") }
                    SpikeLog.event("e", "result", mapOf("result" to json))
                    lines = ffiLines(json)
                    running = false
                }
            }) { Text("Run here") }
        }
        Results("Result", lines)
    }
}
