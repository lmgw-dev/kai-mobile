package dev.rootshell.kai.voice

import android.content.Context
import android.media.AudioAttributes
import android.media.AudioDeviceCallback
import android.media.AudioDeviceInfo
import android.media.AudioFocusRequest
import android.media.AudioManager
import android.os.Handler
import android.os.Looper
import android.util.Log
import dev.rootshell.kai.R
import dev.rootshell.kai.core.ffi.AudioFocus
import dev.rootshell.kai.core.ffi.Route
import java.util.concurrent.Executor
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow

/**
 * Android's focus changes as the call reads them (design §8.5), pure so the JVM tests run them: a
 * loss for good ends the call, a transient one pauses it, the focus back lifts the pause. A loss
 * that only asks to duck (a navigation prompt, a notification's sound) is no loss for a call: the
 * conversation goes on, and Android does not duck speech on its own.
 */
object Focus {
    fun of(change: Int): AudioFocus? = when (change) {
        AudioManager.AUDIOFOCUS_LOSS -> AudioFocus.LOST
        AudioManager.AUDIOFOCUS_LOSS_TRANSIENT -> AudioFocus.LOST_TRANSIENT
        AudioManager.AUDIOFOCUS_GAIN,
        AudioManager.AUDIOFOCUS_GAIN_TRANSIENT,
        AudioManager.AUDIOFOCUS_GAIN_TRANSIENT_EXCLUSIVE,
        AudioManager.AUDIOFOCUS_GAIN_TRANSIENT_MAY_DUCK,
        -> AudioFocus.GAINED
        else -> null
    }
}

/**
 * The call's audio settings (design §8.3, §8.5), on the main thread:
 * - **at a call's start** `MODE_IN_COMMUNICATION`, the communication device by [Routing.pick] (the
 *   first headset, Bluetooth before wired, else `voice.route`), and the audio focus
 *   (`AUDIOFOCUS_GAIN_TRANSIENT` with the voice-communication attributes: music pauses or ducks as its
 *   app decides);
 * - **during the call** the device list follows `AudioDeviceCallback` (a headset connected takes
 *   over, one removed falls back by the rule), Android's own switches follow its communication
 *   device listener, and the route button's choice ([choose]) holds while its device is there. The
 *   state's changes are [RouteState]'s: a call Android moved away from the requested device stays
 *   where Android put it, said, until the next device change; a requested device that does not
 *   arrive within `lmgw.timeout_s` is said in place of "switching…";
 * - **at the end** the focus abandoned, the device cleared, `MODE_NORMAL`.
 *
 * [routes] is what the route button shows. A device Android refuses is said there and logged, never
 * hidden; Android's own choice then stands. Bluetooth needs no `BLUETOOTH_CONNECT` here:
 * `setCommunicationDevice` sets up SCO itself, and the device's name comes with it (design §8.3's
 * note).
 */
class CallAudio(private val context: Context) {
    private val audio = context.getSystemService(AudioManager::class.java)
    private val main = Handler(Looper.getMainLooper())
    private val mainExecutor = Executor { main.post(it) }
    private var route = Route.SPEAKER
    private var running = false
    private var focus: AudioFocusRequest? = null

    /** `lmgw.timeout_s` at the call's start: how long a requested device may take to arrive. */
    private var timeoutMs = 0L
    private var stallCheck: Runnable? = null

    private val _routes = MutableStateFlow(RouteState())
    val routes: StateFlow<RouteState> = _routes

    private val deviceCallback = object : AudioDeviceCallback() {
        override fun onAudioDevicesAdded(addedDevices: Array<out AudioDeviceInfo>) = refresh("a device connected")
        override fun onAudioDevicesRemoved(removedDevices: Array<out AudioDeviceInfo>) = refresh("a device left")
    }

    private val deviceChanged = AudioManager.OnCommunicationDeviceChangedListener { device ->
        if (!running) return@OnCommunicationDeviceChangedListener
        Log.i(TAG, "voice: Android routes the call to ${device?.let { describe(it) } ?: "its default"}")
        val before = _routes.value
        val after = before.routed(device?.id)
        _routes.value = after
        val left = after.left
        if (left != null && before.left == null) {
            Log.i(TAG, "voice: Android moved the call off '${label(left)}'; its choice stands until the next device change")
        }
    }

    /**
     * Asks for the call's audio focus; `onChange` gets Android's changes (on the main thread).
     * False when Android refused it (another app holds the audio, a phone call). A loss for good
     * gives the request up (Android dropped it), so a later start asks again.
     */
    fun requestFocus(onChange: (Int) -> Unit): Boolean {
        focus?.let { return true }
        val request = AudioFocusRequest.Builder(AudioManager.AUDIOFOCUS_GAIN_TRANSIENT)
            .setAudioAttributes(ATTRIBUTES)
            .setOnAudioFocusChangeListener(
                { change ->
                    if (change == AudioManager.AUDIOFOCUS_LOSS) abandonFocus()
                    onChange(change)
                },
                main,
            )
            .build()
        val result = audio.requestAudioFocus(request)
        if (result != AudioManager.AUDIOFOCUS_REQUEST_GRANTED) {
            Log.w(TAG, "voice: Android refused the audio focus ($result)")
            return false
        }
        focus = request
        return true
    }

    fun abandonFocus() {
        focus?.let { audio.abandonAudioFocusRequest(it) }
        focus = null
    }

    /**
     * A call's start: the mode and the route by the rule; a requested device waits at most
     * `timeoutS` (`lmgw.timeout_s`) to arrive. A call that runs already keeps its route (a rebind).
     */
    fun begin(route: Route, timeoutS: UInt) {
        if (running) return
        running = true
        this.route = route
        timeoutMs = timeoutS.toLong() * 1000
        audio.mode = AudioManager.MODE_IN_COMMUNICATION
        // The list first: registering the callback reports every device as added, which must not
        // read as a headset connecting.
        _routes.value = RouteState(devices = devices(), actual = audio.communicationDevice?.id)
        audio.registerAudioDeviceCallback(deviceCallback, main)
        audio.addOnCommunicationDeviceChangedListener(mainExecutor, deviceChanged)
        ask("the call starts")
    }

    /** The route button: device `id` for the rest of the call (while it is there). */
    fun choose(id: Int) {
        if (!running) return
        _routes.value = _routes.value.choosing(id)
        ask("the owner chose it")
    }

    fun end() {
        if (!running) return
        running = false
        stallCheck?.let(main::removeCallbacks)
        stallCheck = null
        audio.unregisterAudioDeviceCallback(deviceCallback)
        audio.removeOnCommunicationDeviceChangedListener(deviceChanged)
        audio.clearCommunicationDevice()
        audio.mode = AudioManager.MODE_NORMAL
        _routes.value = RouteState()
    }

    private fun refresh(why: String) {
        if (!running) return
        _routes.value = _routes.value.withDevices(devices()) ?: return
        ask(why)
    }

    /** Asks Android for the device the rule picks, when it is not the one asked for already. */
    private fun ask(why: String) {
        val state = _routes.value
        if (Routing.pick(state.devices, route, state.chosen) == null) {
            Log.w(TAG, "voice: no communication device to route the call to; Android's choice stands")
            return
        }
        val pick = Routing.toAsk(state, route) ?: return
        val info = audio.availableCommunicationDevices.firstOrNull { it.id == pick.id }
        val ok = info != null && try {
            audio.setCommunicationDevice(info)
        } catch (e: Exception) {
            Log.w(TAG, "voice: setCommunicationDevice threw ${e.javaClass.simpleName}: ${e.message}")
            false
        }
        if (!ok) {
            Log.w(TAG, "voice: Android refused the communication device ${pick.kind} '${label(pick)}' ($why)")
            _routes.value = state.refusing(pick)
            return
        }
        Log.i(TAG, "voice: the call's audio goes to ${pick.kind} '${label(pick)}' ($why)")
        val next = state.asked(pick)
        _routes.value = next
        stallCheck?.let(main::removeCallbacks)
        stallCheck = null
        if (next.switching) {
            val ask = next.asks
            stallCheck = Runnable { timedOut(ask) }.also { main.postDelayed(it, timeoutMs) }
        }
    }

    private fun timedOut(ask: Int) {
        if (!running) return
        val before = _routes.value
        val after = before.timedOut(ask)
        if (after == before) return
        Log.w(
            TAG,
            "voice: Android did not route the call to '${after.stalled?.let { label(it) }}' within lmgw.timeout_s " +
                "(${timeoutMs / 1000} s); its own choice stands until the next device change",
        )
        _routes.value = after
    }

    private fun devices(): List<RouteDevice> = Routing.ordered(
        audio.availableCommunicationDevices.map { d ->
            RouteDevice(d.id, d.type, Routing.kindOf(d.type), d.productName?.toString()?.takeIf { it.isNotBlank() })
        },
    )

    /** A device's name for the route button and the log. */
    fun label(d: RouteDevice): String = when (d.kind) {
        RouteKind.SPEAKER -> context.getString(R.string.route_speaker)
        RouteKind.EARPIECE -> context.getString(R.string.route_earpiece)
        RouteKind.WIRED -> context.getString(R.string.route_wired)
        RouteKind.USB -> d.name ?: context.getString(R.string.route_usb)
        RouteKind.BLUETOOTH -> d.name ?: context.getString(R.string.route_bluetooth)
        RouteKind.HEARING_AID -> d.name ?: context.getString(R.string.route_hearing_aid)
        RouteKind.OTHER -> d.name ?: context.getString(R.string.route_other, d.type)
    }

    private fun describe(d: AudioDeviceInfo): String =
        label(RouteDevice(d.id, d.type, Routing.kindOf(d.type), d.productName?.toString()?.takeIf { it.isNotBlank() }))

    companion object {
        private const val TAG = "kai"

        /** The call's attributes: the focus request's, the earcons' and the MediaSession's. */
        val ATTRIBUTES: AudioAttributes = AudioAttributes.Builder()
            .setUsage(AudioAttributes.USAGE_VOICE_COMMUNICATION)
            .setContentType(AudioAttributes.CONTENT_TYPE_SPEECH)
            .build()
    }
}
