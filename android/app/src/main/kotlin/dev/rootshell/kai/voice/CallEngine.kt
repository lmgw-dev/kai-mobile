package dev.rootshell.kai.voice

import android.content.Context
import android.media.AudioAttributes
import android.media.MediaRecorder
import android.util.Log
import dev.rootshell.kai.core.CoreFacade
import dev.rootshell.kai.core.ffi.CallEnd
import dev.rootshell.kai.core.ffi.CallStart
import dev.rootshell.kai.core.ffi.CoreException
import dev.rootshell.kai.core.ffi.EchoMode
import dev.rootshell.kai.core.ffi.EndReason
import dev.rootshell.kai.core.ffi.EventSink
import dev.rootshell.kai.core.ffi.MicState
import dev.rootshell.kai.core.ffi.PlayoutStats
import dev.rootshell.kai.core.ffi.TransportState
import java.math.BigInteger
import java.nio.ByteBuffer
import java.util.concurrent.Executors
import java.util.concurrent.atomic.AtomicLong
import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.asCoroutineDispatcher
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import org.webrtc.AudioSource
import org.webrtc.AudioTrack
import org.webrtc.DataChannel
import org.webrtc.IceCandidate
import org.webrtc.MediaConstraints
import org.webrtc.MediaStream
import org.webrtc.MediaStreamTrack
import org.webrtc.PeerConnection
import org.webrtc.PeerConnectionFactory
import org.webrtc.RtpReceiver
import org.webrtc.RtpSender
import org.webrtc.RtpTransceiver
import org.webrtc.SdpObserver
import org.webrtc.SessionDescription
import org.webrtc.audio.JavaAudioDeviceModule

/**
 * The call's libwebrtc side (design §7.3, §8.1): LiveKit's libwebrtc build used standalone through
 * `org.webrtc`, as WP0's spike does. One PeerConnectionFactory with its audio device module for
 * the process (prepared when the voice screen shows; no microphone opens until a call's audio
 * starts), one call at a time. Every rule is the core's: this class builds the PeerConnection and
 * its offer, hands libwebrtc's states and the channel's messages to the core, and carries out the
 * core's decisions ([EventSink]) on one thread of its own, in order.
 *
 * - **The PeerConnection:** no ICE servers, unified plan, `max-bundle`, `rtcp-mux` required,
 *   continual gathering (a network change adds candidates, §7.11); a `sendrecv` audio transceiver
 *   with the microphone track and the `oai-events` channel (ordered, reliable) before the offer;
 *   the offer set locally and handed over at once, no gathering waited for (lmgw is ICE-lite).
 * - **The microphone** ([MicState]): `Live` the track enabled, `Silent` disabled (RTP with
 *   silence), `Off` the sender's encoding inactive as well (no capture, no RTP). The device
 *   module's first samples after `Off` reach `Voice::capturing` (the delayed cue, §7.9).
 * - **Kai's track** is muted by disabling the remote track, which sets its output volume to zero.
 * - **The call's number**, which the core answers at the start, goes with every libwebrtc event of
 *   the call, so the core drops an older call's (its PeerConnection may still be closing when the
 *   next call starts: libwebrtc reports `closed` as `close()` begins).
 * - **libwebrtc's failures** (the offer, the answer, a message the open channel refused) end the
 *   call through `Voice::failed` in libwebrtc's words, never as a silent wait for the connect bound.
 *
 * - **The echo setting** (`voice.echo`, §8.2): the factory and its device module are built with the
 *   canceller the setting asks for and rebuilt when that changed, between calls only (never under a
 *   PeerConnection): at the next [prepare], [refreshFactory] or call start.
 * - **The ring's levels** ([levels]): the microphone's from the device module's captured samples,
 *   Kai's from its played ones (§10.3).
 *
 * The call's audio around it (routing, focus, the MediaSession, the earcons) is [VoiceController]'s.
 */
class CallEngine(
    private val context: Context,
    private val core: CoreFacade,
    private val listener: Listener,
) {
    /** What the app does around a call: the service's microphone type, the call's audio, the cue. */
    interface Listener {
        /**
         * Call `call` (the core's number) starts: the microphone type on Kai's service, the call's
         * audio settings.
         */
        fun callStarting(call: ULong)

        /** The call is over for the owner: the microphone type and the call's audio go. */
        fun callEnded(end: CallEnd)

        /** The listening cue (§10.5). */
        fun cue()

        /** A call waits for the owner's verdict in this call (§10.5, §11): the ask's earcon and haptic. */
        fun asked()
    }

    private val thread = Executors.newSingleThreadExecutor { Thread(it, "kai-voice") }
    private val dispatcher = thread.asCoroutineDispatcher()
    private val scope = CoroutineScope(dispatcher)
    private var factory: PeerConnectionFactory? = null
    private var adm: JavaAudioDeviceModule? = null
    /** Whether the factory's device module uses the hardware canceller (§8.2). */
    private var factoryHw: Boolean? = null
    private var initialized = false

    /** The ring's levels (design §10.3), written on libwebrtc's audio threads. */
    val levels = Levels()
    /** The call the core took, and one whose start the core has not answered yet. */
    private var call: Peer? = null
    private var pending: Peer? = null

    /** The calls' numbers and the order the listener hears their starts and ends in. */
    private val order = CallOrder()

    /** The core's number of the call whose first captured samples are awaited (0: none). */
    private val firstSamples = AtomicLong(0)

    /** One call's libwebrtc objects and the core's latest decisions for them. */
    private class Peer(val n: Int) {
        /** The core's number for the call, which its events carry (set once the core took it). */
        var id: ULong = 0uL
        var pc: PeerConnection? = null
        var source: AudioSource? = null
        var track: AudioTrack? = null
        var sender: RtpSender? = null
        var receiver: RtpReceiver? = null
        var channel: DataChannel? = null
        var mic: MicState? = null
        var audible = true
        var closed = false
    }

    /** A rebind's next call (§7.7), started once call `first`'s PeerConnection is closed. */
    private var restart: Pair<Int, CallStart>? = null

    /** The factory and its device module (design §7.2, §8.1), and the warm connection; idempotent. */
    fun prepare() {
        prepareFactory()
        core.voice.prepare()
    }

    /**
     * The factory and its device module alone, no network and no microphone (design §9.2: the
     * assistant's `onReady`); idempotent.
     */
    fun prepareFactory() {
        scope.launch { ensureFactory() }
    }

    /** The factory rebuilt now if `voice.echo` changed (the settings page saved it), unless a call runs. */
    fun refreshFactory() {
        scope.launch { ensureFactory() }
    }

    private fun ensureFactory(): PeerConnectionFactory {
        // §8.2: the hardware canceller where the phone reports one and the setting allows it,
        // else WebRTC's own (AEC3), half duplex included. Only the canceller is the device module's:
        // software and half duplex differ in the session alone, and need no rebuild.
        val echo = core.settings().voice.echo
        val hw = hardwareCanceller(echo, JavaAudioDeviceModule.isBuiltInAcousticEchoCancelerSupported())
        factory?.let { f ->
            // A PeerConnection made by this factory may still live: the change waits for the next call.
            val inUse = call?.let { it.pc != null && !it.closed } == true
            if (hw == factoryHw || inUse) return f
            Log.i(TAG, "voice: voice.echo is $echo now (hardware canceller $hw); the WebRTC factory is rebuilt")
            f.dispose()
            adm?.release()
            factory = null
            adm = null
        }
        if (!initialized) {
            PeerConnectionFactory.initialize(
                PeerConnectionFactory.InitializationOptions.builder(context.applicationContext).createInitializationOptions(),
            )
            initialized = true
        }
        val module = JavaAudioDeviceModule.builder(context.applicationContext)
            .setAudioSource(MediaRecorder.AudioSource.VOICE_COMMUNICATION)
            .setAudioAttributes(
                AudioAttributes.Builder()
                    .setUsage(AudioAttributes.USAGE_VOICE_COMMUNICATION)
                    .setContentType(AudioAttributes.CONTENT_TYPE_SPEECH)
                    .build(),
            )
            .setUseHardwareAcousticEchoCanceler(hw)
            .setUseHardwareNoiseSuppressor(hw)
            .setSamplesReadyCallback { samples ->
                levels.mic = Level.of(samples.data)
                val id = firstSamples.getAndSet(0)
                if (id != 0L) core.voice.capturing(id.toULong())
            }
            // Kai's level from what is played (§10.3): a stop's local mute reads as silence at once.
            .setPlaybackSamplesReadyCallback { samples -> levels.kai = Level.of(samples.data) }
            .createAudioDeviceModule()
        val f = PeerConnectionFactory.builder().setAudioDeviceModule(module).createPeerConnectionFactory()
        adm = module
        factory = f
        factoryHw = hw
        Log.i(TAG, "voice: the WebRTC factory is prepared (echo $echo, hardware canceller $hw)")
        return f
    }

    /**
     * Starts a call (design §7.3) from a visible activity or the assistant's session: the core's
     * checks, then (once the core took it) the microphone type and the call's audio, the
     * PeerConnection and its offer, lmgw's answer, on the voice thread. Answers as soon as the core
     * took the call, with its number, so the assistant overlay can name it at once (a dismissal
     * while the offer is on its way, §9.2); its end comes through the listener with the core's
     * words, also when libwebrtc or lmgw fail it afterwards. Refused, the core's words of why.
     */
    suspend fun start(start: CallStart): Started {
        val n = withContext(dispatcher) {
            val n = order.asked()
            pending = Peer(n)
            n
        }
        val id = try {
            core.voice.start(start, Sink(n))
        } catch (e: CoreException) {
            // Refused (a call runs, the link's state): the running call, if any, is untouched.
            withContext(dispatcher) {
                if (pending?.n == n) pending = null
                order.refused(n)
            }
            return Started.Refused(e.message ?: "voice could not start")
        }
        withContext(dispatcher) {
            // The core took it: an old PeerConnection still closing goes now (its events name the
            // old call, which the core drops).
            call?.let { old -> if (!old.closed) dispose(old) }
            call = (pending?.takeIf { it.n == n } ?: Peer(n)).also { it.id = id }
            pending = null
            // On the voice thread, in the order of the core's decisions: the start before any end
            // (an end the core decided before this ran follows it here).
            order.took(n, id).forEach { hear(n, it) }
        }
        scope.launch { connect(n, id) }
        return Started.Call(id)
    }

    /** Call `n` (the core's `id`): the PeerConnection, its offer POSTed, lmgw's answer set. */
    private suspend fun connect(n: Int, id: ULong) {
        val offer = try {
            createPeer(n)
        } catch (e: Exception) {
            val ended = call?.n != n || call?.closed != false
            if (ended) return
            Log.w(TAG, "voice: the PeerConnection could not be set up: ${e.javaClass.simpleName}: ${e.message}")
            // The call ends in libwebrtc's words, which the voice screen shows as its end.
            core.voice.failed(id, "libwebrtc could not set up the call (${e.message})")
            return
        }
        val answer = try {
            core.voice.offer(id, offer)
        } catch (e: CoreException) {
            // The core has ended the call: its end and the PeerConnection's close come through
            // the sink.
            return
        }
        val peer = call?.takeIf { it.n == n && !it.closed } ?: return
        peer.pc?.setRemoteDescription(AnswerObserver(id), SessionDescription(SessionDescription.Type.ANSWER, answer))
    }

    private suspend fun createPeer(n: Int): String {
        val f = ensureFactory()
        val peer = call?.takeIf { it.n == n && !it.closed } ?: error("the call was replaced")
        val config = PeerConnection.RTCConfiguration(emptyList()).apply {
            sdpSemantics = PeerConnection.SdpSemantics.UNIFIED_PLAN
            bundlePolicy = PeerConnection.BundlePolicy.MAXBUNDLE
            rtcpMuxPolicy = PeerConnection.RtcpMuxPolicy.REQUIRE
            continualGatheringPolicy = PeerConnection.ContinualGatheringPolicy.GATHER_CONTINUALLY
        }
        val pc = f.createPeerConnection(config, Observer(peer)) ?: error("createPeerConnection returned null")
        peer.pc = pc
        val source = f.createAudioSource(MediaConstraints())
        val track = f.createAudioTrack("kai-mic", source)
        peer.source = source
        peer.track = track
        val transceiver = pc.addTransceiver(
            track,
            RtpTransceiver.RtpTransceiverInit(RtpTransceiver.RtpTransceiverDirection.SEND_RECV, listOf("kai")),
        )
        peer.sender = transceiver.sender
        peer.receiver = transceiver.receiver
        applyMic(peer)
        applyPlayback(peer)
        val channel = pc.createDataChannel("oai-events", DataChannel.Init().apply { ordered = true })
            ?: error("createDataChannel returned null")
        channel.registerObserver(ChannelObserver(peer, channel))
        peer.channel = channel
        val offer = CompletableDeferred<SessionDescription>()
        pc.createOffer(
            object : SdpObserver {
                override fun onCreateSuccess(desc: SessionDescription) { offer.complete(desc) }
                override fun onCreateFailure(e: String?) { offer.completeExceptionally(IllegalStateException("createOffer: ${e ?: "no reason given"}")) }
                override fun onSetSuccess() {}
                override fun onSetFailure(e: String?) {}
            },
            MediaConstraints(),
        )
        val desc = offer.await()
        // The core may have ended the call meanwhile (its connect bound, End): the PeerConnection
        // is gone then and must not be touched.
        check(!peer.closed) { "the call ended while its offer was made" }
        val set = CompletableDeferred<Unit>()
        pc.setLocalDescription(
            object : SdpObserver {
                override fun onCreateSuccess(desc: SessionDescription?) {}
                override fun onCreateFailure(e: String?) {}
                override fun onSetSuccess() { set.complete(Unit) }
                override fun onSetFailure(e: String?) { set.completeExceptionally(IllegalStateException("setLocalDescription: ${e ?: "no reason given"}")) }
            },
            desc,
        )
        set.await()
        check(!peer.closed) { "the call ended while its offer was set" }
        // The SDP is not edited (§7.3 step 2).
        return desc.description
    }

    private fun applyMic(peer: Peer) {
        val mic = peer.mic ?: return
        val track = peer.track ?: return
        val sender = peer.sender ?: return
        track.setEnabled(mic == MicState.LIVE)
        val params = sender.parameters
        val active = mic != MicState.OFF
        if (params.encodings.any { it.active != active }) {
            params.encodings.forEach { it.active = active }
            if (!sender.setParameters(params)) Log.w(TAG, "voice: the sender's encoding could not be set ${if (active) "active" else "inactive"}")
        }
    }

    private fun applyPlayback(peer: Peer) {
        peer.receiver?.track()?.setEnabled(peer.audible)
    }

    /** What the listener hears about call `n`, on the voice thread. */
    private fun hear(n: Int, heard: CallOrder.Heard) {
        when (heard) {
            is CallOrder.Heard.Starting -> listener.callStarting(heard.id)
            is CallOrder.Heard.Ended -> listener.callEnded(heard.end)
            // A rebind keeps the microphone type and the call's audio for the next call.
            is CallOrder.Heard.Rebind -> restart = n to heard.start
        }
    }

    private fun dispose(peer: Peer) {
        if (peer.closed) return
        peer.closed = true
        peer.channel?.let {
            it.unregisterObserver()
            it.dispose()
        }
        peer.pc?.let {
            it.close()
            it.dispose()
        }
        // The track the factory made (the sender holds its own reference), then its source.
        peer.track?.dispose()
        peer.source?.dispose()
        peer.channel = null
        peer.pc = null
        peer.source = null
        peer.track = null
        peer.sender = null
        peer.receiver = null
        if (call === peer) call = null
        levels.reset()
        Log.i(TAG, "voice: the PeerConnection of call ${peer.n} is closed")
    }

    /** The core's decisions for call `n` (design §3.2), each posted to the voice thread in order. */
    private inner class Sink(private val n: Int) : EventSink {
        private fun on(block: (Peer) -> Unit) {
            thread.execute {
                // The core may decide before `start` has moved its call from `pending`.
                val peer = (call?.takeIf { it.n == n } ?: pending?.takeIf { it.n == n })
                    ?.takeIf { !it.closed } ?: return@execute
                try {
                    block(peer)
                } catch (e: Exception) {
                    Log.w(TAG, "voice: ${e.javaClass.simpleName}: ${e.message}")
                }
            }
        }

        override fun send(text: String) = on { peer ->
            val channel = peer.channel ?: return@on
            if (channel.send(DataChannel.Buffer(ByteBuffer.wrap(text.toByteArray(Charsets.UTF_8)), false))) return@on
            val state = channel.state()
            Log.w(TAG, "voice: an oai-events message was not sent (the channel is $state)")
            // A closing channel reports its close itself; one that is open lost the message, and
            // the call cannot go on as the core decided it.
            if (state != DataChannel.State.CLOSING && state != DataChannel.State.CLOSED) {
                core.voice.failed(peer.id, "libwebrtc could not send on the events channel (${state.name.lowercase()})")
            }
        }

        override fun setMic(mic: MicState) = on { peer ->
            if (mic != MicState.OFF && (peer.mic == null || peer.mic == MicState.OFF)) firstSamples.set(peer.id.toLong())
            peer.mic = mic
            applyMic(peer)
        }

        override fun setPlayback(audible: Boolean) = on { peer ->
            peer.audible = audible
            applyPlayback(peer)
        }

        override fun readStats(request: ULong) = on { peer ->
            val pc = peer.pc
            val receiver = peer.receiver
            if (pc == null || receiver == null) {
                core.voice.stats(peer.id, request, PlayoutStats(null, null, null))
                return@on
            }
            pc.getStats(receiver) { report ->
                val inbound = report.statsMap.values.firstOrNull {
                    it.type == "inbound-rtp" && it.members["kind"] == "audio"
                }?.members
                core.voice.stats(
                    peer.id,
                    request,
                    PlayoutStats(
                        estimatedPlayoutTimestamp = (inbound?.get("estimatedPlayoutTimestamp") as? Number)?.toDouble(),
                        jitterBufferDelay = (inbound?.get("jitterBufferDelay") as? Number)?.toDouble(),
                        jitterBufferEmittedCount = inbound?.get("jitterBufferEmittedCount")?.let(::unsigned),
                    ),
                )
            }
        }

        override fun closeChannel() = on { peer -> peer.channel?.close() }

        override fun closePeer() = on { peer ->
            dispose(peer)
            val again = restart?.takeIf { it.first == n }?.second ?: return@on
            restart = null
            // A rebind (§7.7): the next call binds by the folder at once.
            scope.launch {
                val why = (start(again) as? Started.Refused)?.words
                if (why != null) {
                    Log.i(TAG, "voice: the rebind did not start: $why")
                    listener.callEnded(CallEnd(EndReason.REFUSED, why, null, null, emptyList(), null))
                }
            }
        }

        override fun cue() {
            thread.execute { if (order.cue(n)) listener.cue() }
        }

        override fun asked() {
            thread.execute { if (order.cue(n)) listener.asked() }
        }

        override fun ended(end: CallEnd) {
            thread.execute { order.ended(n, end)?.let { hear(n, it) } }
        }
    }

    /** libwebrtc's PeerConnection events, each naming the core's call (the core drops another's). */
    private inner class Observer(private val peer: Peer) : PeerConnection.Observer {
        override fun onConnectionChange(state: PeerConnection.PeerConnectionState) {
            val s = when (state) {
                PeerConnection.PeerConnectionState.NEW -> TransportState.NEW
                PeerConnection.PeerConnectionState.CONNECTING -> TransportState.CONNECTING
                PeerConnection.PeerConnectionState.CONNECTED -> TransportState.CONNECTED
                PeerConnection.PeerConnectionState.DISCONNECTED -> TransportState.DISCONNECTED
                PeerConnection.PeerConnectionState.FAILED -> TransportState.FAILED
                PeerConnection.PeerConnectionState.CLOSED -> TransportState.CLOSED
            }
            thread.execute { core.voice.transport(peer.id, s, state.name.lowercase()) }
        }

        override fun onSignalingChange(state: PeerConnection.SignalingState?) {}
        override fun onIceConnectionChange(state: PeerConnection.IceConnectionState?) {
            Log.d(TAG, "voice: ICE $state")
        }
        override fun onIceConnectionReceivingChange(receiving: Boolean) {}
        override fun onIceGatheringChange(state: PeerConnection.IceGatheringState?) {}
        override fun onIceCandidate(candidate: IceCandidate?) {}
        override fun onIceCandidatesRemoved(candidates: Array<out IceCandidate>?) {}
        override fun onAddStream(stream: MediaStream?) {}
        override fun onRemoveStream(stream: MediaStream?) {}
        override fun onDataChannel(channel: DataChannel?) {}
        override fun onRenegotiationNeeded() {}
        override fun onTrack(transceiver: RtpTransceiver?) {
            if (transceiver?.mediaType == MediaStreamTrack.MediaType.MEDIA_TYPE_AUDIO) {
                thread.execute {
                    if (peer.closed || call !== peer) return@execute
                    peer.receiver = transceiver.receiver
                    applyPlayback(peer)
                }
            }
        }
    }

    private inner class ChannelObserver(private val peer: Peer, private val channel: DataChannel) : DataChannel.Observer {
        override fun onBufferedAmountChange(previous: Long) {}

        override fun onStateChange() {
            val state = channel.state()
            thread.execute {
                when (state) {
                    DataChannel.State.OPEN -> core.voice.channelOpen(peer.id)
                    DataChannel.State.CLOSED -> core.voice.channelClosed(peer.id, "closed")
                    else -> {}
                }
            }
        }

        override fun onMessage(buffer: DataChannel.Buffer) {
            if (buffer.binary) {
                Log.w(TAG, "voice: a binary message on oai-events; skipped")
                return
            }
            // The buffer lives only for this callback: read now, handed over in order on the voice
            // thread with the channel's states. The core only enqueues it (it joins parts, §7.10).
            val bytes = ByteArray(buffer.data.remaining())
            buffer.data.get(bytes)
            val text = String(bytes, Charsets.UTF_8)
            thread.execute { core.voice.channelMessage(peer.id, text) }
        }
    }

    /** lmgw's answer set: a failure ends call `id` in libwebrtc's words (never a wait for the connect bound). */
    private inner class AnswerObserver(private val id: ULong) : SdpObserver {
        override fun onCreateSuccess(desc: SessionDescription?) {}
        override fun onCreateFailure(e: String?) {}
        override fun onSetSuccess() {}
        override fun onSetFailure(e: String?) {
            Log.w(TAG, "voice: setting lmgw's answer failed: $e")
            thread.execute {
                core.voice.failed(id, "libwebrtc could not take lmgw's answer (setRemoteDescription: ${e ?: "no reason given"})")
            }
        }
    }

    companion object {
        private const val TAG = "kai"

        /**
         * Whether the device module uses the phone's hardware canceller and noise suppressor
         * (design §8.2): `hardware` yes, `software` and `half_duplex` no (WebRTC's AEC3), `auto` where
         * the phone reports a canceller.
         */
        fun hardwareCanceller(echo: EchoMode, available: Boolean): Boolean = when (echo) {
            EchoMode.HARDWARE -> true
            EchoMode.SOFTWARE, EchoMode.HALF_DUPLEX -> false
            EchoMode.AUTO -> available
        }

        /** A stat's unsigned count, as libwebrtc hands it (a BigInteger or a Long). */
        fun unsigned(v: Any): ULong? = when (v) {
            is BigInteger -> if (v.signum() >= 0) v.toLong().toULong() else null
            is Number -> v.toLong().takeIf { it >= 0 }?.toULong()
            else -> null
        }
    }
}

/**
 * Which call's start and end [CallEngine.Listener] hears, and in what order (design §7.3), pure so
 * the JVM tests run it; on the voice thread. A call gets its number when it is asked for, and
 * becomes current once the core took it. The core may end a call before its start is recorded (its
 * decisions reach the voice thread before the start's answer does): that end waits and follows the
 * start, so the call's audio is always set up before it is released, never left held. An older
 * call's end is not heard; a rebind's end is heard as the rebind (the audio stays).
 */
class CallOrder {
    sealed interface Heard {
        data class Starting(val id: ULong) : Heard

        data class Ended(val end: CallEnd) : Heard

        /** A rebind (§7.7): the next call starts once this one's PeerConnection is closed. */
        data class Rebind(val start: CallStart) : Heard
    }

    private var next = 0

    /** The number of the call the core took (0: none yet). */
    var current = 0
        private set

    /** The number of the call whose start the core has not answered yet (0: none). */
    private var pending = 0

    /** The pending call's end, which came before its start was recorded. */
    private var early: CallEnd? = null

    /** A new call's number. */
    fun asked(): Int {
        next += 1
        pending = next
        early = null
        return next
    }

    /** The core refused call `n`'s start: it never ran. */
    fun refused(n: Int) {
        if (pending != n) return
        pending = 0
        early = null
    }

    /** The core took call `n` as `id`: its start, and its end if that came first. */
    fun took(n: Int, id: ULong): List<Heard> {
        current = n
        val end = early.takeIf { pending == n }
        if (pending == n) {
            pending = 0
            early = null
        }
        return listOfNotNull(Heard.Starting(id), end?.let(::heard))
    }

    /** The core ended call `n`: what the listener hears now (null: nothing yet, or never). */
    fun ended(n: Int, end: CallEnd): Heard? = when (n) {
        current -> heard(end)
        pending -> {
            early = end
            null
        }
        else -> null
    }

    /** Whether call `n`'s cue is heard. */
    fun cue(n: Int): Boolean = n == current

    private fun heard(end: CallEnd): Heard = end.restart?.let { Heard.Rebind(it) } ?: Heard.Ended(end)
}
