package dev.rootshell.kai.spike.service

import android.annotation.SuppressLint
import android.content.Context
import android.media.AudioFormat
import android.media.AudioRecord
import android.media.MediaRecorder
import android.os.PowerManager
import android.os.SystemClock
import dev.rootshell.kai.spike.Device
import dev.rootshell.kai.spike.SpikeLog
import dev.rootshell.kai.spike.r1
import kotlin.math.log10
import kotlin.math.max
import kotlin.math.sqrt

/**
 * A microphone capture held by the service (b, n): 16 kHz mono from VOICE_COMMUNICATION, read in
 * 20 ms frames. It counts samples against the time that passed (lost audio shows as the
 * difference), logs every gap between reads over 100 ms with the screen's state, and every 10 s a
 * tick: the level, whether Android silenced the client, the screen and the posture.
 */
class Capture(private val context: Context, private val onTick: (Stats) -> Unit) {
    data class Stats(
        val running: Boolean,
        val origin: String,
        val startedElapsed: Long,
        val durationMs: Long,
        val samples: Long,
        val expectedSamples: Long,
        val maxGapMs: Double,
        val gapsOver100: Int,
        val lastRmsDb: Double,
        val silencedEver: Boolean,
        val screenOffMs: Long,
        val sampleRate: Int,
        val error: String?,
    ) {
        val lostMs: Double get() = max(0L, expectedSamples - samples) * 1000.0 / sampleRate

        fun toMap(): Map<String, Any?> = mapOf(
            "origin" to origin, "duration_s" to durationMs / 1000.0, "samples" to samples,
            "expected_samples" to expectedSamples, "lost_ms" to lostMs.r1(), "max_gap_ms" to maxGapMs.r1(),
            "gaps_over_100ms" to gapsOver100, "rms_dbfs" to lastRmsDb.r1(), "silenced_ever" to silencedEver,
            "screen_off_s" to screenOffMs / 1000.0, "sample_rate" to sampleRate, "error" to error,
        )
    }

    private val rate = 16000
    @Volatile private var thread: Thread? = null
    @Volatile private var running = false
    @Volatile private var stats = Stats(false, "", 0, 0, 0, 0, 0.0, 0, -120.0, false, 0, rate, null)

    fun stats(): Stats = stats

    @SuppressLint("MissingPermission") // RECORD_AUDIO is checked by the caller; a refusal is logged
    fun start(origin: String) {
        if (running) return
        running = true
        val started = SystemClock.elapsedRealtime()
        stats = Stats(true, origin, started, 0, 0, 0, 0.0, 0, -120.0, false, 0, rate, null)
        thread = Thread({ loop(origin, started) }, "spike-capture").also { it.start() }
    }

    /** Stops and waits for the loop; the final stats. */
    fun stop(): Stats {
        running = false
        thread?.join(2000)
        thread = null
        stats = stats.copy(running = false)
        return stats
    }

    @SuppressLint("MissingPermission")
    private fun loop(origin: String, started: Long) {
        val power = context.getSystemService(PowerManager::class.java)
        val frame = ShortArray(rate / 50)
        val record = try {
            val min = AudioRecord.getMinBufferSize(rate, AudioFormat.CHANNEL_IN_MONO, AudioFormat.ENCODING_PCM_16BIT)
            AudioRecord.Builder()
                .setAudioSource(MediaRecorder.AudioSource.VOICE_COMMUNICATION)
                .setAudioFormat(
                    AudioFormat.Builder().setSampleRate(rate).setChannelMask(AudioFormat.CHANNEL_IN_MONO)
                        .setEncoding(AudioFormat.ENCODING_PCM_16BIT).build(),
                )
                .setBufferSizeInBytes(max(min, rate / 5 * 2))
                .build()
        } catch (e: Exception) {
            fail("${e.javaClass.simpleName}: ${e.message}")
            return
        }
        try {
            record.startRecording()
            if (record.recordingState != AudioRecord.RECORDSTATE_RECORDING) {
                fail("the recording did not start (state ${record.recordingState})")
                return
            }
            var samples = 0L
            var firstNanos = 0L
            var lastNanos = 0L
            var maxGap = 0.0
            var gaps = 0
            var silencedEver = false
            var screenOff = 0L
            var sumSq = 0.0
            var sumN = 0
            var lastTick = SystemClock.elapsedRealtime()
            var lastScreenCheck = lastTick
            var rmsDb = -120.0
            while (running) {
                val n = record.read(frame, 0, frame.size)
                val now = System.nanoTime()
                if (n < 0) {
                    fail("read returned $n")
                    break
                }
                if (firstNanos == 0L) firstNanos = now
                if (lastNanos != 0L) {
                    val gap = (now - lastNanos) / 1e6
                    if (gap > maxGap) maxGap = gap
                    if (gap > 100) {
                        gaps++
                        SpikeLog.event(
                            "b", "gap",
                            mapOf("gap_ms" to gap.r1(), "screen_on" to power.isInteractive, "origin" to origin),
                        )
                    }
                }
                lastNanos = now
                samples += n
                for (i in 0 until n) sumSq += frame[i].toDouble() * frame[i]
                sumN += n
                val el = SystemClock.elapsedRealtime()
                if (el - lastScreenCheck >= 1000) {
                    if (!power.isInteractive) screenOff += el - lastScreenCheck
                    lastScreenCheck = el
                }
                if (el - lastTick >= 10_000) {
                    rmsDb = if (sumN > 0) 20 * log10(max(sqrt(sumSq / sumN), 1e-6) / 32768.0) else -120.0
                    sumSq = 0.0
                    sumN = 0
                    lastTick = el
                    val silenced = runCatching { record.activeRecordingConfiguration?.isClientSilenced }.getOrNull()
                    if (silenced == true) silencedEver = true
                    val expected = ((now - firstNanos) / 1e9 * rate).toLong()
                    stats = Stats(true, origin, started, el - started, samples, expected, maxGap, gaps, rmsDb,
                        silencedEver, screenOff, rate, null)
                    SpikeLog.event(
                        "b", "capture_tick",
                        stats.toMap() + mapOf(
                            "silenced_now" to silenced, "screen_on" to power.isInteractive,
                            "posture" to Device.posture(context),
                        ),
                    )
                    onTick(stats)
                }
            }
            val expected = if (firstNanos == 0L) 0 else ((lastNanos - firstNanos) / 1e9 * rate).toLong() + frame.size
            stats = Stats(false, origin, started, SystemClock.elapsedRealtime() - started, samples, expected, maxGap,
                gaps, rmsDb, silencedEver, screenOff, rate, stats.error)
        } catch (e: Exception) {
            fail("${e.javaClass.simpleName}: ${e.message}")
        } finally {
            runCatching { record.stop() }
            record.release()
        }
    }

    private fun fail(why: String) {
        running = false
        stats = stats.copy(running = false, error = why)
        SpikeLog.event("b", "capture_error", mapOf("error" to why))
    }
}
