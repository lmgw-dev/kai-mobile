package dev.rootshell.kai.spike

import android.content.Context
import android.os.Process
import android.os.SystemClock
import android.util.Log
import java.io.File
import java.io.IOException
import java.time.OffsetDateTime
import java.time.format.DateTimeFormatter
import java.util.concurrent.Executors
import java.util.concurrent.TimeUnit
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import org.json.JSONArray
import org.json.JSONObject

/**
 * The spike's log: one JSON object per line in `wp0/wp0.jsonl` under the app's external files
 * directory (`just spike-log` pulls it; spikes/wp0/fill-results.py reads it), every line also to
 * logcat under the tag `kai-spike`, and a tail in memory for the screens.
 *
 * The file is written on a thread of its own, so logging on a measured path (an overlay's first
 * frame, a capture's read loop) costs no disk I/O there.
 *
 * Each line carries `ts` (wall clock), `elapsed_ms` (SystemClock.elapsedRealtime), `pid`, `proc`,
 * `point` ("a" … "o", or "app") and `event`, then the event's own fields.
 */
object SpikeLog {
    const val TAG = "kai-spike"

    /** How many lines the screens show; the file keeps every line. */
    const val TAIL = 400

    data class Entry(val wallMs: Long, val point: String, val event: String, val text: String)

    private val tail = MutableStateFlow<List<Entry>>(emptyList())
    val entries: StateFlow<List<Entry>> = tail

    @Volatile private var file: File? = null
    @Volatile private var process: String = "main"
    private val lock = Any()
    private val writer = Executors.newSingleThreadExecutor { Thread(it, "spike-log").apply { isDaemon = true } }

    fun init(context: Context, process: String) {
        this.process = process
        file = File(dir(context), "wp0.jsonl")
    }

    /** `wp0/` under the external files directory (adb reads it), else under internal storage. */
    fun dir(context: Context): File =
        File(context.getExternalFilesDir(null) ?: context.filesDir, "wp0").apply { mkdirs() }

    fun path(): String = file?.path ?: "(not initialised)"

    fun event(point: String, event: String, fields: Map<String, Any?> = emptyMap(), text: String? = null) {
        val json = JSONObject()
        json.put("ts", OffsetDateTime.now().format(DateTimeFormatter.ISO_OFFSET_DATE_TIME))
        json.put("elapsed_ms", SystemClock.elapsedRealtime())
        json.put("pid", Process.myPid())
        json.put("proc", process)
        json.put("point", point)
        json.put("event", event)
        for ((k, v) in fields) json.put(k, toJson(v))
        val summary = text ?: fields.entries.joinToString(" ") { "${it.key}=${short(it.value)}" }
        Log.i(TAG, "$point.$event $summary")
        val line = json.toString() + "\n"
        val target = file
        writer.execute {
            try {
                target?.appendText(line)
            } catch (e: IOException) {
                Log.w(TAG, "cannot write ${target?.path}", e)
            }
        }
        synchronized(lock) {
            tail.value = (tail.value + Entry(System.currentTimeMillis(), point, event, summary)).takeLast(TAIL)
        }
    }

    /** Waits until every line logged so far is in the file (before the process ends itself). */
    fun flush() {
        runCatching { writer.submit {}.get(2, TimeUnit.SECONDS) }
    }

    /** A value as org.json takes it: maps, lists and records become objects and arrays. */
    fun toJson(v: Any?): Any = when (v) {
        null -> JSONObject.NULL
        is JSONObject, is JSONArray, is String, is Boolean, is Int, is Long -> v
        is Double -> if (v.isNaN() || v.isInfinite()) JSONObject.NULL else v
        is Float -> toJson(v.toDouble())
        is Number -> v.toDouble()
        is Map<*, *> -> JSONObject().also { o -> v.forEach { (k, x) -> o.put(k.toString(), toJson(x)) } }
        is Iterable<*> -> JSONArray().also { a -> v.forEach { a.put(toJson(it)) } }
        is Array<*> -> toJson(v.toList())
        is IntArray -> toJson(v.toList())
        is Enum<*> -> v.name
        else -> v.toString()
    }

    private fun short(v: Any?): String = when (v) {
        is Double -> if (v.isNaN()) "NaN" else String.format(java.util.Locale.ROOT, "%.2f", v)
        is Map<*, *> -> "{…}"
        is Iterable<*> -> "[…]"
        else -> v.toString()
    }
}

/** Monotonic milliseconds since `startNanos` (System.nanoTime). */
fun msSince(startNanos: Long): Double = (System.nanoTime() - startNanos) / 1e6

fun Double.r1(): Double = Math.round(this * 10.0) / 10.0

fun Double.r2(): Double = Math.round(this * 100.0) / 100.0
