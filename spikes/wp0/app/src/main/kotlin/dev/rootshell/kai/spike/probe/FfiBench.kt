package dev.rootshell.kai.spike.probe

import android.app.Service
import android.content.Context
import android.content.Intent
import android.content.pm.ApplicationInfo
import android.os.Bundle
import android.os.IBinder
import android.os.Process
import android.os.ResultReceiver
import com.sun.jna.CallbackThreadInitializer
import com.sun.jna.Native
import dev.rootshell.kai.core.UniffiCoreFacade
import dev.rootshell.kai.spike.Device
import dev.rootshell.kai.spike.SpikeApp
import dev.rootshell.kai.spike.SpikeLog
import dev.rootshell.kai.spike.ffi.EventSink
import dev.rootshell.kai.spike.ffi.Timings
import dev.rootshell.kai.spike.ffi.keystoreBench
import dev.rootshell.kai.spike.ffi.sinkBench
import dev.rootshell.kai.spike.ffi.spikeInfo
import dev.rootshell.kai.spike.ffi.tlsBench
import dev.rootshell.kai.spike.ffi.uniffiCallbackInterfaceEventSink
import dev.rootshell.kai.spike.ffi.uniffiCallbackInterfaceKeystore
import dev.rootshell.kai.spike.msSince
import dev.rootshell.kai.spike.r2
import java.io.File
import java.security.Signature
import java.util.UUID
import java.util.concurrent.atomic.AtomicLong
import kotlin.concurrent.thread
import org.json.JSONObject

/**
 * (e)'s measurements (design §17 WP0 (e)): the APK and its libraries, the first call into the core
 * (JNA's own library, then the core's: load, registration, `Core::new`), an `EventSink` call from
 * three kinds of thread, Keystore signatures through the callback (the first cold), and full TLS
 * 1.3 handshakes with the Keystore key as client certificate. Run in a fresh process
 * ([FfiProbeService]) every figure is a cold process's.
 */
object FfiBench {
    /** Whether this process has loaded the spike's library before (then nothing here is cold). */
    @Volatile private var loaded = false

    fun run(context: Context, keepAttached: Boolean, where: String): JSONObject {
        val out = linkedMapOf<String, Any?>(
            "where" to where, "process_age_ms" to SpikeApp.processAgeMs(), "loaded_before" to loaded,
            "keep_attached" to keepAttached, "page_size" to Device.pageSize(),
        )
        try {
            out += apk(context)
            if (keepAttached) {
                // Before the spike's library registers its callbacks: tokio's threads stay attached
                // to the JVM after a callback instead of attaching and detaching every time.
                val init = CallbackThreadInitializer(true, false, "spike-jna")
                Native.setCallbackThreadInitializer(uniffiCallbackInterfaceEventSink.`send`, init)
                Native.setCallbackThreadInitializer(uniffiCallbackInterfaceKeystore.`generate`, init)
                Native.setCallbackThreadInitializer(uniffiCallbackInterfaceKeystore.`sign`, init)
                Native.setCallbackThreadInitializer(uniffiCallbackInterfaceKeystore.`delete`, init)
                out["keep_attached_effective"] = !loaded
            }
            var t = System.nanoTime()
            @Suppress("UNUSED_VARIABLE") val jna = Native.POINTER_SIZE
            out["jna_init_ms"] = msSince(t).r2()
            t = System.nanoTime()
            val core = UniffiCoreFacade.create(context)
            out["core_first_call_ms"] = msSince(t).r2()
            t = System.nanoTime()
            out["core_version"] = core.version()
            out["core_second_call_us"] = (msSince(t) * 1000).r2()
            t = System.nanoTime()
            repeat(1000) { core.version() }
            out["core_call_mean_us"] = (msSince(t)).r2()
            t = System.nanoTime()
            val info = spikeInfo()
            out["spike_first_call_ms"] = msSince(t).r2()
            out["spike_profile"] = info.profile
            out["tokio_workers"] = info.tokioWorkers.toInt()
            loaded = true

            val calls = AtomicLong()
            val sink = object : EventSink {
                override fun send(text: String) {
                    calls.incrementAndGet()
                }
            }
            t = System.nanoTime()
            val s = sinkBench(sink, 200u)
            out["sink"] = mapOf(
                "calling_thread" to timings(s.callingThread), "worker_first_call" to timings(s.workerFirstCall),
                "worker_repeat_call" to timings(s.workerRepeatCall), "fresh_thread" to timings(s.freshThread),
                "workers_used" to s.workersUsed.toInt(), "payload_bytes" to s.payloadBytes.toInt(),
                "calls_seen" to calls.get(), "total_ms" to msSince(t).r2(),
                // The attach and detach a tokio worker pays per call, against an attached thread.
                "attach_detach_us" to (s.workerRepeatCall.medianUs - s.callingThread.medianUs).r2(),
            )

            val keystore = AndroidKeystore()
            val ks = keystoreBench(keystore, "kai-spike-ks-" + UUID.randomUUID(), 20u)
            out["keystore"] = mapOf(
                "generate_ms" to (ks.generateUs / 1000).r2(), "spki_bytes" to ks.spkiBytes.toInt(),
                "sign" to timings(ks.sign), "sign_warm" to timings(ks.signWarm), "verified" to ks.verified.toInt(),
                "signature_bytes" to ks.signatureBytes.toInt(),
                "security_level" to AndroidKeystore.levelName(keystore.lastSecurityLevel),
            )
            out["keystore_direct"] = direct()

            val tls = tlsBench(AndroidKeystore(), "kai-spike-tls-" + UUID.randomUUID(), 10u)
            out["tls"] = mapOf(
                "key_generate_ms" to (tls.keyGenerateUs / 1000).r2(), "ca_ms" to (tls.caUs / 1000).r2(),
                "server_leaf_ms" to (tls.serverLeafUs / 1000).r2(), "device_cert_ms" to (tls.deviceCertUs / 1000).r2(),
                "handshake" to timings(tls.handshake), "handshake_warm" to timings(tls.handshakeWarm),
                "sign" to timings(tls.sign), "protocol" to tls.protocol, "cipher_suite" to tls.cipherSuite,
                "key_exchange" to tls.keyExchange, "client_cert_verified" to tls.clientCertVerified,
                "handshakes" to tls.handshakes.map {
                    mapOf("connect_us" to it.connectUs.r2(), "handshake_us" to it.handshakeUs.r2(),
                        "sign_us" to it.signUs.r2(), "round_trip_us" to it.roundTripUs.r2())
                },
            )
            out["ok"] = true
        } catch (t: Throwable) {
            out["ok"] = false
            out["error"] = "${t.javaClass.name}: ${t.message}"
        }
        val json = SpikeLog.toJson(out) as JSONObject
        return json
    }

    private fun timings(t: Timings): Map<String, Any?> = mapOf(
        "count" to t.count.toInt(), "first_us" to t.firstUs.r2(), "min_us" to t.minUs.r2(),
        "median_us" to t.medianUs.r2(), "p90_us" to t.p90Us.r2(), "max_us" to t.maxUs.r2(), "mean_us" to t.meanUs.r2(),
    )

    /** The same signatures straight from Kotlin, no FFI: what the callback adds. */
    private fun direct(): Map<String, Any?> {
        val ks = AndroidKeystore()
        val alias = "kai-spike-direct-" + UUID.randomUUID()
        return try {
            ks.generate(alias)
            val msg = ByteArray(130) { it.toByte() }
            val us = (0 until 20).map {
                val t = System.nanoTime()
                ks.sign(alias, msg)
                msSince(t) * 1000
            }
            val warm = us.drop(1).sorted()
            mapOf("first_us" to us.first().r2(), "warm_median_us" to warm[warm.size / 2].r2())
        } catch (e: Exception) {
            mapOf("error" to e.message)
        } finally {
            ks.delete(alias)
        }
    }

    /** The installed APK's size and its libraries' alignment, read on the phone. */
    fun apk(context: Context): Map<String, Any?> {
        val info = context.applicationInfo
        val base = File(info.sourceDir)
        val splits = info.splitSourceDirs.orEmpty().map { File(it) }
        val libs = (listOf(base) + splits).flatMap { runCatching { ApkCheck.libs(it) }.getOrDefault(emptyList()) }
        return mapOf(
            "apk_bytes" to base.length() + splits.sumOf { it.length() },
            "apk_splits" to splits.size,
            "extract_native_libs" to (info.flags and ApplicationInfo.FLAG_EXTRACT_NATIVE_LIBS != 0),
            "libs" to libs.map { it.toMap() },
            "libs_all_16k" to libs.all { it.ok },
        )
    }
}

/**
 * Runs [FfiBench] in the `:ffiprobe` process, which nothing else uses: started fresh for every run,
 * answers through the [ResultReceiver] it was given and then ends its process.
 */
class FfiProbeService : Service() {
    override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int {
        @Suppress("DEPRECATION")
        val receiver = intent?.getParcelableExtra<ResultReceiver>(EXTRA_RECEIVER)
        val keep = intent?.getBooleanExtra(EXTRA_KEEP_ATTACHED, false) ?: false
        thread(name = "spike-ffi-probe") {
            val json = FfiBench.run(applicationContext, keep, "fresh_process")
            receiver?.send(0, Bundle().apply { putString(EXTRA_JSON, json.toString()) })
            stopSelf()
            // The next run must find no warm process.
            SpikeLog.flush()
            Thread.sleep(200)
            Process.killProcess(Process.myPid())
        }
        return START_NOT_STICKY
    }

    override fun onBind(intent: Intent?): IBinder? = null

    companion object {
        const val EXTRA_RECEIVER = "receiver"
        const val EXTRA_KEEP_ATTACHED = "keep_attached"
        const val EXTRA_JSON = "json"
    }
}
