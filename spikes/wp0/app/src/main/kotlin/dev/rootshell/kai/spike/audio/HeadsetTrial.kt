package dev.rootshell.kai.spike.audio

import android.annotation.SuppressLint
import android.content.Context
import android.content.Intent
import android.media.AudioAttributes
import android.media.AudioDeviceCallback
import android.media.AudioDeviceInfo
import android.media.AudioFocusRequest
import android.media.AudioFormat
import android.media.AudioManager
import android.media.AudioRecord
import android.media.AudioTrack
import android.media.MediaRecorder
import android.media.session.MediaSession
import android.media.session.PlaybackState
import android.os.Build
import android.os.Handler
import android.os.Looper
import android.view.InputDevice
import android.view.KeyEvent
import dev.rootshell.kai.spike.SpikeLog
import dev.rootshell.kai.spike.r1
import java.io.File
import java.time.LocalDateTime
import java.time.format.DateTimeFormatter
import java.util.concurrent.Executors

/**
 * (i): headset buttons through a MediaSession (design §8.4) and Bluetooth's hands-free audio
 * (§8.3). The button test holds what a call holds: an active session in the playing state, audio
 * playing (silence) with the chosen usage, the audio focus, and MODE_IN_COMMUNICATION when asked;
 * every key the session receives is logged with its device. The Bluetooth test routes the call
 * audio to the headset with setCommunicationDevice, times the SCO setup, records a few seconds and
 * reads the sample rates and the audio's bandwidth.
 */
class HeadsetTrial(private val context: Context) {
    private val am = context.getSystemService(AudioManager::class.java)
    private val main = Handler(Looper.getMainLooper())
    private var session: MediaSession? = null
    private var silence: Thread? = null
    @Volatile private var playing = false
    private var focus: AudioFocusRequest? = null
    private var savedMode = AudioManager.MODE_NORMAL

    private val devices = object : AudioDeviceCallback() {
        override fun onAudioDevicesAdded(added: Array<out AudioDeviceInfo>) =
            added.forEach { SpikeLog.event("i", "device_added", describe(it)) }

        override fun onAudioDevicesRemoved(removed: Array<out AudioDeviceInfo>) =
            removed.forEach { SpikeLog.event("i", "device_removed", describe(it)) }
    }

    val running: Boolean get() = session != null

    /** Starts the button test; `voiceUsage` plays as a call (else as media), `communication` sets the mode. */
    fun startButtons(voiceUsage: Boolean, communication: Boolean) {
        if (session != null) return
        val config = mapOf("usage" to if (voiceUsage) "voice_communication" else "media", "mode_in_communication" to communication)
        savedMode = am.mode
        if (communication) am.mode = AudioManager.MODE_IN_COMMUNICATION
        val attrs = AudioAttributes.Builder()
            .setUsage(if (voiceUsage) AudioAttributes.USAGE_VOICE_COMMUNICATION else AudioAttributes.USAGE_MEDIA)
            .setContentType(if (voiceUsage) AudioAttributes.CONTENT_TYPE_SPEECH else AudioAttributes.CONTENT_TYPE_MUSIC)
            .build()
        focus = AudioFocusRequest.Builder(AudioManager.AUDIOFOCUS_GAIN_TRANSIENT)
            .setAudioAttributes(attrs)
            .setOnAudioFocusChangeListener({ SpikeLog.event("i", "focus", mapOf("change" to it)) }, main)
            .build()
        val granted = am.requestAudioFocus(focus!!)
        session = MediaSession(context, "kai-spike").apply {
            setCallback(
                object : MediaSession.Callback() {
                    override fun onMediaButtonEvent(intent: Intent): Boolean {
                        val ev = if (Build.VERSION.SDK_INT >= 33) {
                            intent.getParcelableExtra(Intent.EXTRA_KEY_EVENT, KeyEvent::class.java)
                        } else {
                            @Suppress("DEPRECATION") intent.getParcelableExtra(Intent.EXTRA_KEY_EVENT)
                        }
                        SpikeLog.event("i", "button", keyFields(ev) + config)
                        return true
                    }
                },
                main,
            )
            setPlaybackState(
                PlaybackState.Builder()
                    .setState(PlaybackState.STATE_PLAYING, 0, 1f)
                    .setActions(
                        PlaybackState.ACTION_PLAY_PAUSE or PlaybackState.ACTION_PLAY or PlaybackState.ACTION_PAUSE or
                            PlaybackState.ACTION_STOP or PlaybackState.ACTION_SKIP_TO_NEXT or PlaybackState.ACTION_SKIP_TO_PREVIOUS,
                    )
                    .build(),
            )
            isActive = true
        }
        playing = true
        silence = Thread({ playSilence(attrs) }, "spike-silence").also { it.start() }
        am.registerAudioDeviceCallback(devices, main)
        SpikeLog.event(
            "i", "buttons_started",
            config + mapOf("focus" to granted, "devices" to am.getDevices(AudioManager.GET_DEVICES_INPUTS or AudioManager.GET_DEVICES_OUTPUTS).map { describe(it) }),
        )
    }

    fun stopButtons() {
        playing = false
        silence?.join(1000)
        silence = null
        session?.run {
            isActive = false
            release()
        }
        session = null
        focus?.let { am.abandonAudioFocusRequest(it) }
        focus = null
        am.unregisterAudioDeviceCallback(devices)
        am.mode = savedMode
        SpikeLog.event("i", "buttons_stopped")
    }

    private fun playSilence(attrs: AudioAttributes) {
        val rate = 48000
        val track = AudioTrack.Builder()
            .setAudioAttributes(attrs)
            .setAudioFormat(
                AudioFormat.Builder().setSampleRate(rate).setChannelMask(AudioFormat.CHANNEL_OUT_MONO)
                    .setEncoding(AudioFormat.ENCODING_PCM_16BIT).build(),
            )
            .setTransferMode(AudioTrack.MODE_STREAM)
            .build()
        val zeros = ShortArray(rate / 50)
        try {
            track.play()
            while (playing) track.write(zeros, 0, zeros.size)
        } finally {
            runCatching { track.stop() }
            track.release()
        }
    }

    data class Hfp(
        val device: String,
        val accepted: Boolean,
        val changedMs: Double?,
        val routedMs: Double?,
        val firstSpeechMs: Double?,
        val clientRate: Int,
        val deviceFormatRate: Int?,
        val deviceRates: List<Int>,
        val shareAbove4k: Double?,
        val shareAbove7k: Double?,
        val wav: String?,
        val error: String?,
    ) {
        val bandwidth: String
            get() = when {
                shareAbove4k == null -> "no speech heard"
                shareAbove4k < 0.02 -> "narrowband (8 kHz: CVSD)"
                (shareAbove7k ?: 0.0) < 0.01 -> "wideband (16 kHz: mSBC)"
                else -> "wider than 16 kHz (LC3 or the phone's microphone)"
            }

        fun toMap(): Map<String, Any?> = mapOf(
            "device" to device, "accepted" to accepted, "sco_changed_ms" to changedMs, "routed_ms" to routedMs,
            "first_speech_ms" to firstSpeechMs, "client_rate" to clientRate, "device_format_rate" to deviceFormatRate,
            "device_rates" to deviceRates, "share_above_4k" to shareAbove4k, "share_above_7k" to shareAbove7k,
            "bandwidth" to bandwidth, "wav" to wav, "error" to error,
        )
    }

    /** Routes to the Bluetooth headset, records `seconds` s (the owner speaks), restores. */
    @SuppressLint("MissingPermission")
    fun hfp(seconds: Int = 6): Hfp {
        val bt = am.availableCommunicationDevices.firstOrNull {
            it.type == AudioDeviceInfo.TYPE_BLUETOOTH_SCO || it.type == AudioDeviceInfo.TYPE_BLE_HEADSET
        } ?: return Hfp("none", false, null, null, null, 0, null, emptyList(), null, null, null,
            "no Bluetooth headset among the communication devices: connect one")
        val name = describe(bt)
        val rate = 16000
        val saved = am.mode
        val executor = Executors.newSingleThreadExecutor()
        var changedNanos = 0L
        var routedNanos = 0L
        val listener = AudioManager.OnCommunicationDeviceChangedListener { dev ->
            if (dev?.id == bt.id && changedNanos == 0L) changedNanos = System.nanoTime()
            SpikeLog.event("i", "communication_device", mapOf("device" to dev?.let { describe(it) }))
        }
        val record = AudioRecord.Builder()
            .setAudioSource(MediaRecorder.AudioSource.VOICE_COMMUNICATION)
            .setAudioFormat(
                AudioFormat.Builder().setSampleRate(rate).setChannelMask(AudioFormat.CHANNEL_IN_MONO)
                    .setEncoding(AudioFormat.ENCODING_PCM_16BIT).build(),
            )
            .setBufferSizeInBytes(rate)
            .build()
        record.addOnRoutingChangedListener({ r ->
            val dev = r.routedDevice
            SpikeLog.event("i", "record_routed", mapOf("device" to dev?.let { describe(it) }))
            if (dev != null && (dev.type == AudioDeviceInfo.TYPE_BLUETOOTH_SCO || dev.type == AudioDeviceInfo.TYPE_BLE_HEADSET) && routedNanos == 0L) {
                routedNanos = System.nanoTime()
            }
        }, main)
        val samples = ShortArray(rate * seconds)
        var firstSpeechNanos = 0L
        var formatRate: Int? = null
        var accepted = false
        val t0: Long
        try {
            am.addOnCommunicationDeviceChangedListener(executor, listener)
            am.mode = AudioManager.MODE_IN_COMMUNICATION
            t0 = System.nanoTime()
            accepted = am.setCommunicationDevice(bt)
            record.startRecording()
            var done = 0
            val frame = EnergyVad.FRAME
            while (done < samples.size) {
                val n = record.read(samples, done, minOf(frame, samples.size - done))
                if (n <= 0) break
                if (firstSpeechNanos == 0L && routedNanos != 0L && Pcm.db(samples, done, n) > -45) firstSpeechNanos = System.nanoTime()
                done += n
                if (formatRate == null) formatRate = runCatching { record.activeRecordingConfiguration?.format?.sampleRate }.getOrNull()
            }
        } finally {
            runCatching { record.stop() }
            record.release()
            am.clearCommunicationDevice()
            am.mode = saved
            am.removeOnCommunicationDeviceChangedListener(listener)
            executor.shutdown()
        }
        fun since(n: Long) = if (n == 0L) null else ((n - t0) / 1e6).r1()
        val dir = File(SpikeLog.dir(context), "hfp").apply { mkdirs() }
        val wav = File(dir, LocalDateTime.now().format(DateTimeFormatter.ofPattern("yyyyMMdd-HHmmss")) + ".wav")
        Pcm.writeWav(wav, samples, rate)
        val result = Hfp(
            name["type"].toString() + " " + name["product"], accepted, since(changedNanos), since(routedNanos),
            since(firstSpeechNanos), record.sampleRate, formatRate, bt.sampleRates.toList(),
            Spectrum.shareAbove(samples, 4000.0, -45.0), Spectrum.shareAbove(samples, 7000.0, -45.0), wav.name, null,
        )
        SpikeLog.event("i", "hfp", result.toMap())
        return result
    }

    companion object {
        fun typeName(type: Int): String = when (type) {
            AudioDeviceInfo.TYPE_BUILTIN_SPEAKER -> "speaker"
            AudioDeviceInfo.TYPE_BUILTIN_EARPIECE -> "earpiece"
            AudioDeviceInfo.TYPE_BUILTIN_MIC -> "builtin_mic"
            AudioDeviceInfo.TYPE_WIRED_HEADSET -> "wired_headset"
            AudioDeviceInfo.TYPE_WIRED_HEADPHONES -> "wired_headphones"
            AudioDeviceInfo.TYPE_USB_HEADSET -> "usb_headset"
            AudioDeviceInfo.TYPE_USB_DEVICE -> "usb_device"
            AudioDeviceInfo.TYPE_BLUETOOTH_SCO -> "bluetooth_sco"
            AudioDeviceInfo.TYPE_BLUETOOTH_A2DP -> "bluetooth_a2dp"
            AudioDeviceInfo.TYPE_BLE_HEADSET -> "ble_headset"
            AudioDeviceInfo.TYPE_BLE_SPEAKER -> "ble_speaker"
            AudioDeviceInfo.TYPE_HEARING_AID -> "hearing_aid"
            AudioDeviceInfo.TYPE_TELEPHONY -> "telephony"
            else -> "type_$type"
        }

        fun describe(d: AudioDeviceInfo): Map<String, Any?> = mapOf(
            "type" to typeName(d.type), "product" to d.productName?.toString(), "id" to d.id,
            "source" to d.isSource, "sink" to d.isSink, "rates" to d.sampleRates.toList(),
        )

        fun keyFields(ev: KeyEvent?): Map<String, Any?> {
            if (ev == null) return mapOf("key" to null)
            return mapOf(
                "key" to KeyEvent.keyCodeToString(ev.keyCode), "action" to if (ev.action == KeyEvent.ACTION_DOWN) "down" else "up",
                "repeat" to ev.repeatCount, "long_press" to ev.isLongPress, "event_time" to ev.eventTime,
                "input_device" to InputDevice.getDevice(ev.deviceId)?.name, "source" to ev.source,
            )
        }
    }
}
