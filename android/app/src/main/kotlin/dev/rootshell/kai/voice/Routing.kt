package dev.rootshell.kai.voice

import android.media.AudioDeviceInfo
import dev.rootshell.kai.core.ffi.Route

/** What kind of output a communication device is (design §8.3), by `AudioDeviceInfo`'s type. */
enum class RouteKind { BLUETOOTH, HEARING_AID, WIRED, USB, EARPIECE, SPEAKER, OTHER }

/**
 * A device a call can be routed to: Android's id and type, its kind, and the name Android gives a
 * headset (`productName`; a Bluetooth headset's name, which Android hands over without
 * `BLUETOOTH_CONNECT`, else the kind's own label).
 */
data class RouteDevice(val id: Int, val type: Int, val kind: RouteKind, val name: String?)

/**
 * The call's route as the voice screen's route button shows it (design §8.3), and its changes, pure
 * so the JVM tests run them: the devices in the order the button lists them, the owner's choice for
 * this call (none: the rule picks), what Kai asked Android for, what Android routes to now (a
 * Bluetooth headset's SCO link takes a moment), and what the button says under it: a device Android
 * refused, one it moved the call away from, one it never switched to.
 *
 * - **Kai asks** ([asked]): the device is `requested` until Android routes there (`arrived`), which
 *   may be at once.
 * - **Android moves the call away** after it had arrived ([routed]: SCO dropped, a multipoint
 *   headset busy with another phone, the headset's own button ending HFP's virtual call): the
 *   request is cleared, Android's choice stands and is said ("Android moved the call to …"), and the
 *   rule picks again at the next device change or the owner's choice. Kai does not fight Android
 *   over the route in between.
 * - **The requested device never arrives** within `lmgw.timeout_s` ([timedOut]; the device leaving
 *   the list is Android's own signal, [withDevices]): the request is cleared and said in place of
 *   "switching…", as the move is.
 */
data class RouteState(
    val devices: List<RouteDevice> = emptyList(),
    val chosen: Int? = null,
    val requested: Int? = null,
    val actual: Int? = null,
    val refused: RouteDevice? = null,
    /** Android has routed to [requested] since Kai asked for it. */
    val arrived: Boolean = false,
    /** The device Android moved the call away from after it had arrived there. */
    val left: RouteDevice? = null,
    /** The device Kai asked for that Android did not switch to within `lmgw.timeout_s`. */
    val stalled: RouteDevice? = null,
    /** How many times Kai asked during the call: a timeout bounds the ask it was set for. */
    val asks: Int = 0,
) {
    /** The device the button names: what Kai asked for, else what Android routes to. */
    val current: RouteDevice?
        get() = device(requested) ?: device(actual)

    /** What Android routes to now, when it is in the list. */
    val routed: RouteDevice?
        get() = device(actual)

    /** Kai asked for a device Android has not switched to yet. */
    val switching: Boolean
        get() = requested != null && !arrived

    private fun device(id: Int?): RouteDevice? = id?.let { i -> devices.firstOrNull { it.id == i } }

    /**
     * The device list changed to `after` (null: the same devices). A headset that connected takes
     * over, a chosen device that left is no choice any more; a refused or requested device that
     * left is forgotten (the rule picks again).
     */
    fun withDevices(after: List<RouteDevice>): RouteState? {
        if (after.map { it.id } == devices.map { it.id }) return null
        val stillRequested = requested?.takeIf { id -> after.any { it.id == id } }
        return copy(
            devices = after,
            chosen = Routing.chosenAfter(chosen, devices, after),
            requested = stillRequested,
            arrived = arrived && stillRequested != null,
            refused = refused?.takeIf { r -> after.any { it.id == r.id } },
        )
    }

    /** The route button's choice. */
    fun choosing(id: Int): RouteState = copy(chosen = id)

    /** Android took the request for `device`: switching until it routes there (at once if it does). */
    fun asked(device: RouteDevice): RouteState = copy(
        requested = device.id,
        arrived = actual == device.id,
        refused = null,
        left = null,
        stalled = null,
        asks = asks + 1,
    )

    /** Android refused `device`: said under the button, its own choice stands. */
    fun refusing(device: RouteDevice): RouteState = copy(refused = device)

    /**
     * Android's communication device changed to `id` (null: its default). A device said as left or
     * never switched to that Android routes to after all is said no more.
     */
    fun routed(id: Int?): RouteState {
        val now = copy(actual = id, left = left?.takeIf { it.id != id }, stalled = stalled?.takeIf { it.id != id })
        return when {
            requested == null -> now
            id == requested -> now.copy(arrived = true)
            // Arrived once and moved away: Android's choice stands until the next device change.
            arrived -> now.copy(requested = null, arrived = false, left = device(requested))
            // Not there yet (SCO's setup may report another device first): still switching.
            else -> now
        }
    }

    /** Ask number `ask`'s bound (`lmgw.timeout_s`) passed: a device not arrived by then is said. */
    fun timedOut(ask: Int): RouteState =
        if (ask != asks || requested == null || arrived) {
            this
        } else {
            copy(requested = null, stalled = device(requested))
        }
}

/**
 * The routing rules of design §8.3, pure so the JVM tests run them: at a call's start the first of a
 * Bluetooth headset (SCO, LE audio, hearing aid), a wired or USB headset, else `voice.route`; a
 * headset connected mid-call takes over; one removed falls back by the same rule; the route
 * button's choice holds for the call while its device is there.
 */
object Routing {
    fun kindOf(type: Int): RouteKind = when (type) {
        AudioDeviceInfo.TYPE_BLUETOOTH_SCO, AudioDeviceInfo.TYPE_BLE_HEADSET, AudioDeviceInfo.TYPE_BLE_SPEAKER -> RouteKind.BLUETOOTH
        AudioDeviceInfo.TYPE_HEARING_AID -> RouteKind.HEARING_AID
        AudioDeviceInfo.TYPE_WIRED_HEADSET, AudioDeviceInfo.TYPE_WIRED_HEADPHONES -> RouteKind.WIRED
        AudioDeviceInfo.TYPE_USB_HEADSET, AudioDeviceInfo.TYPE_USB_DEVICE -> RouteKind.USB
        AudioDeviceInfo.TYPE_BUILTIN_EARPIECE -> RouteKind.EARPIECE
        AudioDeviceInfo.TYPE_BUILTIN_SPEAKER, AudioDeviceInfo.TYPE_BUILTIN_SPEAKER_SAFE -> RouteKind.SPEAKER
        else -> RouteKind.OTHER
    }

    /** A headset: taken at the start before `voice.route`, and taking over when it connects. */
    fun headset(kind: RouteKind): Boolean =
        kind == RouteKind.BLUETOOTH || kind == RouteKind.HEARING_AID || kind == RouteKind.WIRED || kind == RouteKind.USB

    /** §8.3's order: Bluetooth and hearing aids, then wired and USB, then the phone's own. */
    private fun rank(kind: RouteKind): Int = when (kind) {
        RouteKind.BLUETOOTH, RouteKind.HEARING_AID -> 0
        RouteKind.WIRED, RouteKind.USB -> 1
        RouteKind.EARPIECE -> 2
        RouteKind.SPEAKER -> 3
        RouteKind.OTHER -> 4
    }

    /** The devices in the order the route button lists them (Android's order within a rank). */
    fun ordered(devices: List<RouteDevice>): List<RouteDevice> = devices.sortedBy { rank(it.kind) }

    /**
     * The device for the call: the owner's choice while it is there, else the first headset, else
     * `voice.route`'s, else the first Android offers (none when it offers none: Android's choice
     * stands).
     */
    fun pick(devices: List<RouteDevice>, route: Route, chosen: Int?): RouteDevice? {
        devices.firstOrNull { it.id == chosen }?.let { return it }
        val sorted = ordered(devices)
        sorted.firstOrNull { headset(it.kind) }?.let { return it }
        val fallback = when (route) {
            Route.SPEAKER -> RouteKind.SPEAKER
            Route.EARPIECE -> RouteKind.EARPIECE
        }
        return devices.firstOrNull { it.kind == fallback } ?: sorted.firstOrNull()
    }

    /**
     * The device to ask Android for in `state`: the rule's pick when Kai has not asked for it (none:
     * nothing to ask, or nothing to pick).
     */
    fun toAsk(state: RouteState, route: Route): RouteDevice? =
        pick(state.devices, route, state.chosen)?.takeIf { it.id != state.requested }

    /**
     * The owner's choice after the device list changed from `before` to `after`: a headset that
     * connected takes over (it becomes the choice); a chosen device that left is no choice any more.
     */
    fun chosenAfter(chosen: Int?, before: List<RouteDevice>, after: List<RouteDevice>): Int? {
        val added = after.filter { a -> before.none { it.id == a.id } }
        ordered(added).firstOrNull { headset(it.kind) }?.let { return it.id }
        return chosen?.takeIf { id -> after.any { it.id == id } }
    }
}
