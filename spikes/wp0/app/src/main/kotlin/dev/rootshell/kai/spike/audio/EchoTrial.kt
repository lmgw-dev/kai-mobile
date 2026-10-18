package dev.rootshell.kai.spike.audio

import android.annotation.SuppressLint
import android.content.Context
import android.media.AudioAttributes
import android.media.AudioDeviceInfo
import android.media.AudioFormat
import android.media.AudioManager
import android.media.AudioRecord
import android.media.AudioTrack
import android.media.MediaRecorder
import android.media.audiofx.AcousticEchoCanceler
import android.media.audiofx.NoiseSuppressor
import dev.rootshell.kai.spike.Device
import dev.rootshell.kai.spike.SpikeLog
import dev.rootshell.kai.spike.r1
import java.io.File
import java.time.LocalDateTime
import java.time.format.DateTimeFormatter

/**
 * (d): hardware against software echo cancelling on the speaker (design §8.2). The clip plays
 * through the speaker as a call's audio (MODE_IN_COMMUNICATION, USAGE_VOICE_COMMUNICATION, the
 * built-in speaker as the communication device) at three voice-call volumes while the phone
 * captures, once per mode:
 * - **hardware:** VOICE_COMMUNICATION with AcousticEchoCanceler and NoiseSuppressor enabled;
 * - **raw:** UNPROCESSED (or VOICE_RECOGNITION) with no effect, then an NLMS filter over the known
 *   clip as the software stand-in.
 * The owner stays silent; every onset the energy VAD finds while the clip plays is a false
 * barge-in. Each capture is kept as a WAV for a real VAD later.
 */
object EchoTrial {
    enum class Mode { HARDWARE, RAW }

    data class Run(
        val mode: Mode,
        val volumeIndex: Int,
        val volumeMax: Int,
        val posture: String,
        val source: String,
        val aecAvailable: Boolean,
        val aecEnabled: Boolean?,
        val noiseFloorDb: Double,
        val thresholdDb: Double,
        val echoMedianDb: Double,
        val echoMaxDb: Double,
        val bargeIns: Int,
        val framesAbovePct: Double,
        val softwareBargeIns: Int?,
        val softwareMedianDb: Double?,
        val delayMs: Double?,
        val wav: String,
    ) {
        fun toMap(): Map<String, Any?> = mapOf(
            "mode" to mode.name.lowercase(), "volume_index" to volumeIndex, "volume_max" to volumeMax,
            "posture" to posture, "source" to source, "aec_available" to aecAvailable, "aec_enabled" to aecEnabled,
            "noise_floor_db" to noiseFloorDb.r1(), "threshold_db" to thresholdDb.r1(),
            "echo_median_db" to echoMedianDb.r1(), "echo_max_db" to echoMaxDb.r1(), "barge_ins" to bargeIns,
            "frames_above_pct" to framesAbovePct.r1(), "software_barge_ins" to softwareBargeIns,
            "software_median_db" to softwareMedianDb?.r1(), "delay_ms" to delayMs?.r1(), "wav" to wav,
        )

        override fun toString(): String =
            "${mode.name.lowercase()} vol $volumeIndex/$volumeMax: $bargeIns false barge-ins, echo median ${echoMedianDb.r1()} dB" +
                (softwareBargeIns?.let { " · NLMS: $it, median ${softwareMedianDb?.r1()} dB" } ?: "")
    }

    /** The three volumes: a third, two thirds and all of the voice-call range. */
    fun volumes(am: AudioManager): List<Int> {
        val max = am.getStreamMaxVolume(AudioManager.STREAM_VOICE_CALL)
        val min = am.getStreamMinVolume(AudioManager.STREAM_VOICE_CALL)
        return listOf(min + (max - min) / 3, min + 2 * (max - min) / 3, max).distinct()
    }

    @SuppressLint("MissingPermission")
    fun run(context: Context, progress: (String) -> Unit): List<Run> {
        val am = context.getSystemService(AudioManager::class.java)
        val dir = File(SpikeLog.dir(context), "echo").apply { mkdirs() }
        val clip = Clip.load(SpikeLog.dir(context))
        Pcm.writeWav(File(dir, "clip-16k.wav"), clip.samples)
        val stamp = LocalDateTime.now().format(DateTimeFormatter.ofPattern("yyyyMMdd-HHmmss"))
        val posture = Device.posture(context)
        val savedMode = am.mode
        val savedVolume = am.getStreamVolume(AudioManager.STREAM_VOICE_CALL)
        val results = mutableListOf<Run>()
        SpikeLog.event(
            "d", "start",
            mapOf("clip" to clip.name, "posture" to posture, "saved_volume" to savedVolume,
                "aec_available" to AcousticEchoCanceler.isAvailable(), "ns_available" to NoiseSuppressor.isAvailable(),
                "unprocessed" to am.getProperty(AudioManager.PROPERTY_SUPPORT_AUDIO_SOURCE_UNPROCESSED)),
        )
        try {
            am.mode = AudioManager.MODE_IN_COMMUNICATION
            val speaker = am.availableCommunicationDevices.firstOrNull { it.type == AudioDeviceInfo.TYPE_BUILTIN_SPEAKER }
            val routed = speaker != null && am.setCommunicationDevice(speaker)
            SpikeLog.event("d", "route", mapOf("speaker" to routed, "device" to am.communicationDevice?.type))
            val max = am.getStreamMaxVolume(AudioManager.STREAM_VOICE_CALL)
            for (v in volumes(am)) {
                am.setStreamVolume(AudioManager.STREAM_VOICE_CALL, v, 0)
                Thread.sleep(300)
                for (mode in Mode.entries) {
                    progress("${mode.name.lowercase()} at volume $v/$max …")
                    val r = runOne(context, am, mode, v, max, posture, clip.samples, File(dir, "$stamp-$posture-${mode.name.lowercase()}-v$v.wav"))
                    SpikeLog.event("d", "run", r.toMap())
                    results += r
                }
            }
        } finally {
            am.setStreamVolume(AudioManager.STREAM_VOICE_CALL, savedVolume, 0)
            am.clearCommunicationDevice()
            am.mode = savedMode
        }
        SpikeLog.event("d", "summary", mapOf("posture" to posture, "runs" to results.map { it.toMap() }))
        return results
    }

    private fun rawSource(am: AudioManager): Int =
        if (am.getProperty(AudioManager.PROPERTY_SUPPORT_AUDIO_SOURCE_UNPROCESSED) == "true") {
            MediaRecorder.AudioSource.UNPROCESSED
        } else {
            MediaRecorder.AudioSource.VOICE_RECOGNITION
        }

    @SuppressLint("MissingPermission")
    private fun runOne(
        context: Context,
        am: AudioManager,
        mode: Mode,
        volume: Int,
        volumeMax: Int,
        posture: String,
        clip: ShortArray,
        wav: File,
    ): Run {
        val rate = Pcm.RATE
        val source = if (mode == Mode.HARDWARE) MediaRecorder.AudioSource.VOICE_COMMUNICATION else rawSource(am)
        val record = AudioRecord.Builder()
            .setAudioSource(source)
            .setAudioFormat(
                AudioFormat.Builder().setSampleRate(rate).setChannelMask(AudioFormat.CHANNEL_IN_MONO)
                    .setEncoding(AudioFormat.ENCODING_PCM_16BIT).build(),
            )
            .setBufferSizeInBytes(maxOf(AudioRecord.getMinBufferSize(rate, AudioFormat.CHANNEL_IN_MONO, AudioFormat.ENCODING_PCM_16BIT), rate))
            .build()
        var aec: AcousticEchoCanceler? = null
        var ns: NoiseSuppressor? = null
        if (mode == Mode.HARDWARE) {
            if (AcousticEchoCanceler.isAvailable()) aec = AcousticEchoCanceler.create(record.audioSessionId)?.apply { enabled = true }
            if (NoiseSuppressor.isAvailable()) ns = NoiseSuppressor.create(record.audioSessionId)?.apply { enabled = true }
        }
        val lead = rate
        val tail = rate / 2
        val captured = ShortArray(lead + clip.size + tail)
        val track = AudioTrack.Builder()
            .setAudioAttributes(
                AudioAttributes.Builder().setUsage(AudioAttributes.USAGE_VOICE_COMMUNICATION)
                    .setContentType(AudioAttributes.CONTENT_TYPE_SPEECH).build(),
            )
            .setAudioFormat(
                AudioFormat.Builder().setSampleRate(rate).setChannelMask(AudioFormat.CHANNEL_OUT_MONO)
                    .setEncoding(AudioFormat.ENCODING_PCM_16BIT).build(),
            )
            // Streamed, so a clip of any length plays (a static buffer is bounded by the shared heap).
            .setTransferMode(AudioTrack.MODE_STREAM)
            .build()
        var writer: Thread? = null
        try {
            record.startRecording()
            read(record, captured, 0, lead)
            track.play()
            writer = Thread({
                var at = 0
                while (at < clip.size) {
                    val n = track.write(clip, at, minOf(rate / 10, clip.size - at))
                    if (n <= 0) break
                    at += n
                }
            }, "spike-echo-play").also { it.start() }
            read(record, captured, lead, clip.size + tail)
        } finally {
            writer?.join(2000)
            runCatching { track.stop() }
            track.release()
            runCatching { record.stop() }
            record.release()
            aec?.release()
            ns?.release()
        }
        Pcm.writeWav(wav, captured)

        val dbs = EnergyVad.frameDbs(captured)
        val leadFrames = lead / EnergyVad.FRAME
        val noise = EnergyVad.median(dbs, 5, leadFrames)
        val threshold = maxOf(noise + 15.0, -50.0)
        val playFrom = leadFrames
        val playTo = (lead + clip.size) / EnergyVad.FRAME + 15
        val vad = EnergyVad(threshold).run(dbs, playFrom, playTo)
        val echo = dbs.copyOfRange(playFrom, minOf(playTo, dbs.size))
        var software: Int? = null
        var softwareMedian: Double? = null
        var delayMs: Double? = null
        if (mode == Mode.RAW) {
            val mic = Pcm.toFloat(captured)
            val ref = FloatArray(captured.size).also { r -> for (i in clip.indices) r[lead + i] = clip[i] / 32768f }
            val d = Nlms.delay(mic.copyOfRange(lead, captured.size), ref.copyOfRange(lead, captured.size), rate * 6 / 10)
            delayMs = d * 1000.0 / rate
            val residual = Nlms.cancel(mic, ref, maxOf(0, d - rate / 125))
            Pcm.writeWav(File(wav.parentFile, wav.nameWithoutExtension + "-nlms.wav"), Pcm.toShort(residual))
            val rdbs = EnergyVad.frameDbs(residual)
            software = EnergyVad(threshold).run(rdbs, playFrom, playTo).onsets
            softwareMedian = EnergyVad.median(rdbs, playFrom, playTo)
        }
        return Run(
            mode, volume, volumeMax, posture,
            if (source == MediaRecorder.AudioSource.VOICE_COMMUNICATION) "VOICE_COMMUNICATION"
            else if (source == MediaRecorder.AudioSource.UNPROCESSED) "UNPROCESSED" else "VOICE_RECOGNITION",
            AcousticEchoCanceler.isAvailable(), aec?.enabled,
            noise, threshold, EnergyVad.median(echo, 0, echo.size), echo.maxOrNull() ?: -120.0,
            vad.onsets, if (vad.frames == 0) 0.0 else 100.0 * vad.framesAbove / vad.frames,
            software, softwareMedian, delayMs, wav.name,
        )
    }

    private fun read(record: AudioRecord, into: ShortArray, from: Int, count: Int) {
        var done = 0
        while (done < count) {
            val n = record.read(into, from + done, minOf(EnergyVad.FRAME, count - done))
            if (n <= 0) error("AudioRecord.read returned $n")
            done += n
        }
    }
}
