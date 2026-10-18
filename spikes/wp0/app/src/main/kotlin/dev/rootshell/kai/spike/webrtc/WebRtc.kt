package dev.rootshell.kai.spike.webrtc

import android.content.Context
import android.media.AudioAttributes
import android.media.MediaRecorder
import dev.rootshell.kai.spike.msSince
import dev.rootshell.kai.spike.r2
import java.util.concurrent.CountDownLatch
import java.util.concurrent.TimeUnit
import org.webrtc.DataChannel
import org.webrtc.IceCandidate
import org.webrtc.MediaConstraints
import org.webrtc.MediaStream
import org.webrtc.MediaStreamTrack
import org.webrtc.PeerConnection
import org.webrtc.PeerConnectionFactory
import org.webrtc.RtpTransceiver
import org.webrtc.SdpObserver
import org.webrtc.SessionDescription
import org.webrtc.audio.JavaAudioDeviceModule

/**
 * LiveKit's libwebrtc build used standalone through org.webrtc (design §7.2, §8.1): the factory
 * and its device module, prepared once per process (the assistant's `onReady`, design §9.2) and
 * kept. Preparing opens no microphone.
 */
object WebRtc {
    data class Prep(
        val initializeMs: Double,
        val admMs: Double,
        val factoryMs: Double,
        val alreadyPrepared: Boolean,
        val hwAecSupported: Boolean,
        val hwNsSupported: Boolean,
        val platformProcessing: String?,
        val error: String?,
    ) {
        fun toMap(): Map<String, Any?> = mapOf(
            "initialize_ms" to initializeMs, "adm_ms" to admMs, "factory_ms" to factoryMs,
            "already_prepared" to alreadyPrepared, "hw_aec_supported" to hwAecSupported,
            "hw_ns_supported" to hwNsSupported, "platform_processing" to platformProcessing, "error" to error,
        )
    }

    @Volatile var factory: PeerConnectionFactory? = null
        private set
    @Volatile var adm: JavaAudioDeviceModule? = null
        private set
    @Volatile var firstPrep: Prep? = null
        private set

    @Synchronized
    fun prepare(context: Context): Prep {
        val existing = factory
        if (existing != null) {
            return (firstPrep ?: Prep(0.0, 0.0, 0.0, true, false, false, null, null)).copy(alreadyPrepared = true)
        }
        return try {
            var t = System.nanoTime()
            PeerConnectionFactory.initialize(
                PeerConnectionFactory.InitializationOptions.builder(context.applicationContext).createInitializationOptions(),
            )
            val init = msSince(t)
            t = System.nanoTime()
            val module = JavaAudioDeviceModule.builder(context.applicationContext)
                .setAudioSource(MediaRecorder.AudioSource.VOICE_COMMUNICATION)
                .setAudioAttributes(
                    AudioAttributes.Builder()
                        .setUsage(AudioAttributes.USAGE_VOICE_COMMUNICATION)
                        .setContentType(AudioAttributes.CONTENT_TYPE_SPEECH)
                        .build(),
                )
                .setUseHardwareAcousticEchoCanceler(true)
                .setUseHardwareNoiseSuppressor(true)
                .createAudioDeviceModule()
            val admMs = msSince(t)
            t = System.nanoTime()
            val f = PeerConnectionFactory.builder().setAudioDeviceModule(module).createPeerConnectionFactory()
            val factoryMs = msSince(t)
            adm = module
            factory = f
            val platform = runCatching {
                module.platformAudioProcessingState?.let {
                    "topology=${it.topology} aec=${state(it.echoCancellation)} ns=${state(it.noiseSuppression)} agc=${state(it.autoGainControl)}"
                }
            }.getOrElse { "unreadable: ${it.javaClass.simpleName}" }
            Prep(
                init.r2(), admMs.r2(), factoryMs.r2(), false,
                JavaAudioDeviceModule.isBuiltInAcousticEchoCancelerSupported(),
                JavaAudioDeviceModule.isBuiltInNoiseSuppressorSupported(),
                platform, null,
            ).also { firstPrep = it }
        } catch (t: Throwable) {
            Prep(0.0, 0.0, 0.0, false, false, false, null, "${t.javaClass.name}: ${t.message}")
        }
    }

    private fun state(c: JavaAudioDeviceModule.PlatformAudioProcessingComponentState?) =
        if (c == null) "?" else "available=${c.isAvailable} requested=${c.isRequested} active=${c.isActive}"

    data class Offer(val ms: Double, val sdpLines: Int, val hasAudio: Boolean, val opus: Boolean, val error: String?)

    /**
     * A PeerConnection with one audio transceiver and no ICE servers creates an offer (no
     * gathering waited for), then is disposed: libwebrtc's signalling works. No track, so no
     * microphone.
     */
    fun offer(timeoutMs: Long = 5000): Offer {
        val f = factory ?: return Offer(0.0, 0, false, false, "the factory is not prepared")
        val t = System.nanoTime()
        val config = PeerConnection.RTCConfiguration(emptyList()).apply {
            sdpSemantics = PeerConnection.SdpSemantics.UNIFIED_PLAN
        }
        val pc = f.createPeerConnection(config, NoopObserver) ?: return Offer(0.0, 0, false, false, "createPeerConnection returned null")
        try {
            pc.addTransceiver(
                MediaStreamTrack.MediaType.MEDIA_TYPE_AUDIO,
                RtpTransceiver.RtpTransceiverInit(RtpTransceiver.RtpTransceiverDirection.SEND_RECV),
            )
            val done = CountDownLatch(1)
            var sdp: String? = null
            var error: String? = null
            pc.createOffer(
                object : SdpObserver {
                    override fun onCreateSuccess(desc: SessionDescription) {
                        sdp = desc.description
                        done.countDown()
                    }

                    override fun onCreateFailure(e: String?) {
                        error = e ?: "failed"
                        done.countDown()
                    }

                    override fun onSetSuccess() {}
                    override fun onSetFailure(e: String?) {}
                },
                MediaConstraints(),
            )
            if (!done.await(timeoutMs, TimeUnit.MILLISECONDS)) error = "no offer within $timeoutMs ms"
            val text = sdp.orEmpty()
            return Offer(
                msSince(t).r2(), text.lines().count { it.isNotBlank() },
                text.contains("m=audio"), text.contains("opus/48000"), error,
            )
        } finally {
            pc.dispose()
        }
    }

    private object NoopObserver : PeerConnection.Observer {
        override fun onSignalingChange(state: PeerConnection.SignalingState?) {}
        override fun onIceConnectionChange(state: PeerConnection.IceConnectionState?) {}
        override fun onIceConnectionReceivingChange(receiving: Boolean) {}
        override fun onIceGatheringChange(state: PeerConnection.IceGatheringState?) {}
        override fun onIceCandidate(candidate: IceCandidate?) {}
        override fun onIceCandidatesRemoved(candidates: Array<out IceCandidate>?) {}
        override fun onAddStream(stream: MediaStream?) {}
        override fun onRemoveStream(stream: MediaStream?) {}
        override fun onDataChannel(channel: DataChannel?) {}
        override fun onRenegotiationNeeded() {}
    }
}
