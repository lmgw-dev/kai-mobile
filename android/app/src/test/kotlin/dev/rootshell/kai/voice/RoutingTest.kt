package dev.rootshell.kai.voice

import android.media.AudioDeviceInfo
import dev.rootshell.kai.core.ffi.Route
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

class RoutingTest {
    private fun device(id: Int, type: Int, name: String? = null) = RouteDevice(id, type, Routing.kindOf(type), name)

    private val earpiece = device(1, AudioDeviceInfo.TYPE_BUILTIN_EARPIECE)
    private val speaker = device(2, AudioDeviceInfo.TYPE_BUILTIN_SPEAKER)
    private val wired = device(3, AudioDeviceInfo.TYPE_WIRED_HEADSET)
    private val usb = device(4, AudioDeviceInfo.TYPE_USB_HEADSET, "USB-C to 3.5mm")
    private val sco = device(5, AudioDeviceInfo.TYPE_BLUETOOTH_SCO, "Example Buds")
    private val le = device(6, AudioDeviceInfo.TYPE_BLE_HEADSET, "Example Buds LE")
    private val aid = device(7, AudioDeviceInfo.TYPE_HEARING_AID, "Example Hearing")

    @Test
    fun kindsByAndroidsType() {
        assertEquals(RouteKind.BLUETOOTH, sco.kind)
        assertEquals(RouteKind.BLUETOOTH, le.kind)
        assertEquals(RouteKind.HEARING_AID, aid.kind)
        assertEquals(RouteKind.WIRED, Routing.kindOf(AudioDeviceInfo.TYPE_WIRED_HEADPHONES))
        assertEquals(RouteKind.USB, usb.kind)
        assertEquals(RouteKind.EARPIECE, earpiece.kind)
        assertEquals(RouteKind.SPEAKER, speaker.kind)
        assertEquals(RouteKind.OTHER, Routing.kindOf(AudioDeviceInfo.TYPE_TELEPHONY))
        assertTrue(Routing.headset(RouteKind.HEARING_AID))
        assertFalse(Routing.headset(RouteKind.SPEAKER))
    }

    @Test
    fun withoutAHeadsetTheSettingDecides() {
        val phone = listOf(earpiece, speaker)
        assertEquals(speaker, Routing.pick(phone, Route.SPEAKER, chosen = null))
        assertEquals(earpiece, Routing.pick(phone, Route.EARPIECE, chosen = null))
    }

    @Test
    fun aBluetoothHeadsetComesBeforeAWiredOneAndBothBeforeTheSetting() {
        assertEquals(wired, Routing.pick(listOf(earpiece, speaker, wired), Route.SPEAKER, null))
        assertEquals(usb, Routing.pick(listOf(speaker, usb), Route.SPEAKER, null))
        assertEquals(sco, Routing.pick(listOf(earpiece, speaker, wired, sco), Route.SPEAKER, null))
        assertEquals(aid, Routing.pick(listOf(speaker, wired, aid), Route.EARPIECE, null))
    }

    @Test
    fun theOwnersChoiceHoldsWhileItsDeviceIsThere() {
        val all = listOf(earpiece, speaker, sco)
        assertEquals(speaker, Routing.pick(all, Route.EARPIECE, chosen = speaker.id))
        // A choice whose device is gone falls back by the rule.
        assertEquals(sco, Routing.pick(all, Route.SPEAKER, chosen = 99))
    }

    @Test
    fun nothingOfferedLeavesAndroidsChoice() {
        assertNull(Routing.pick(emptyList(), Route.SPEAKER, null))
        // Neither the setting's device nor a headset: the first Android offers.
        val other = device(9, AudioDeviceInfo.TYPE_TELEPHONY)
        assertEquals(other, Routing.pick(listOf(other), Route.SPEAKER, null))
    }

    @Test
    fun aHeadsetConnectedMidCallTakesOverAndOneRemovedFallsBack() {
        val before = listOf(earpiece, speaker)
        // The owner chose the earpiece; a Bluetooth headset connects: it takes over.
        val withBt = before + sco
        val chosen = Routing.chosenAfter(earpiece.id, before, withBt)
        assertEquals(sco.id, chosen)
        assertEquals(sco, Routing.pick(withBt, Route.SPEAKER, chosen))
        // It leaves: no choice any more, the setting decides again.
        val after = Routing.chosenAfter(chosen, withBt, before)
        assertNull(after)
        assertEquals(speaker, Routing.pick(before, Route.SPEAKER, after))
        // A wired headset left while Bluetooth stays: Bluetooth by the rule.
        val both = listOf(speaker, wired, sco)
        assertEquals(sco, Routing.pick(listOf(speaker, sco), Route.SPEAKER, Routing.chosenAfter(wired.id, both, listOf(speaker, sco))))
    }

    @Test
    fun aDeviceThatIsNoHeadsetDoesNotTakeOver() {
        val before = listOf(earpiece)
        assertEquals(earpiece.id, Routing.chosenAfter(earpiece.id, before, before + speaker))
    }

    @Test
    fun theButtonListsHeadsetsFirstThenThePhonesOwn() {
        assertEquals(
            // Within a rank, Android's order.
            listOf(sco, aid, usb, wired, earpiece, speaker),
            Routing.ordered(listOf(speaker, earpiece, usb, wired, sco, aid)),
        )
    }

    /** A call on the phone's own devices, `voice.route` speaker: Kai asked for the speaker, Android routes there. */
    private fun onTheSpeaker(): RouteState {
        val start = RouteState(devices = Routing.ordered(listOf(earpiece, speaker)), actual = speaker.id)
        val ask = Routing.toAsk(start, Route.SPEAKER)
        assertEquals(speaker, ask)
        // Android routes there already: no "switching…".
        return start.asked(ask!!).also { assertFalse(it.switching) }
    }

    /** A Bluetooth headset connects mid-call and Kai asks for it. */
    private fun scoAsked(): RouteState {
        val state = onTheSpeaker().withDevices(Routing.ordered(listOf(earpiece, speaker, sco)))!!
        assertEquals(sco.id, state.chosen)
        val ask = Routing.toAsk(state, Route.SPEAKER)
        assertEquals(sco, ask)
        return state.asked(ask!!)
    }

    @Test
    fun scoRequestedTakesAMomentThenAndroidRoutesElsewhere() {
        var state = scoAsked()
        assertTrue(state.switching)
        assertEquals(sco, state.current)
        // SCO's setup may report another device first: still switching.
        state = state.routed(earpiece.id)
        assertTrue(state.switching)
        state = state.routed(sco.id)
        assertFalse(state.switching)
        assertEquals(sco, state.current)
        // SCO dropped (a multipoint headset busy, the headset's button ended HFP's call): Android
        // moves the call to the speaker. The request is cleared, the move is said, nothing switches.
        state = state.routed(speaker.id)
        assertNull(state.requested)
        assertFalse(state.switching)
        assertEquals(sco, state.left)
        assertEquals(speaker, state.routed)
        assertEquals(speaker, state.current)
        // Kai does not fight Android: the same devices are no device change, nothing is asked.
        assertNull(state.withDevices(state.devices))
        // The next device change picks again: the headset, still there, is asked for again.
        val other = device(9, AudioDeviceInfo.TYPE_TELEPHONY)
        val changed = state.withDevices(Routing.ordered(state.devices + other))!!
        val ask = Routing.toAsk(changed, Route.SPEAKER)
        assertEquals(sco, ask)
        val asked = changed.asked(ask!!)
        assertNull(asked.left)
        assertTrue(asked.switching)
    }

    @Test
    fun androidMovingTheCallToTheDefaultSaysTheDeviceItLeft() {
        val state = scoAsked().routed(sco.id).routed(null)
        assertEquals(sco, state.left)
        assertNull(state.routed)
        assertNull(state.current)
        // Android routes back to the headset by itself: said no more.
        val back = state.routed(sco.id)
        assertNull(back.left)
        assertEquals(sco, back.current)
    }

    @Test
    fun theRequestedDeviceNeverArrives() {
        val state = scoAsked().routed(earpiece.id)
        // An older ask's bound changes nothing.
        assertEquals(state, state.timedOut(state.asks - 1))
        // lmgw.timeout_s passed: said in place of "switching…", Android's choice stands.
        val timedOut = state.timedOut(state.asks)
        assertFalse(timedOut.switching)
        assertNull(timedOut.requested)
        assertEquals(sco, timedOut.stalled)
        assertEquals(earpiece, timedOut.current)
        // A bound after the device arrived changes nothing.
        val arrived = state.routed(sco.id)
        assertEquals(arrived, arrived.timedOut(arrived.asks))
        // It arrives late: said no more.
        assertNull(timedOut.routed(sco.id).stalled)
        // The owner's choice asks again.
        val chosen = timedOut.choosing(sco.id)
        assertEquals(sco, Routing.toAsk(chosen, Route.SPEAKER))
        assertNull(chosen.asked(sco).stalled)
    }

    @Test
    fun aRequestedDeviceThatLeavesIsForgottenAndTheRulePicksAgain() {
        val state = scoAsked().withDevices(Routing.ordered(listOf(earpiece, speaker)))!!
        assertNull(state.requested)
        assertFalse(state.switching)
        assertEquals(speaker, Routing.toAsk(state, Route.SPEAKER))
    }

    @Test
    fun refusedThenTheDeviceLeaves() {
        val connected = onTheSpeaker().withDevices(Routing.ordered(listOf(earpiece, speaker, sco)))!!
        val refused = connected.refusing(sco)
        assertEquals(sco, refused.refused)
        // Android's choice stands: the button names what it asked for before.
        assertEquals(speaker, refused.current)
        // Another device change (no headset) asks again while the headset is there.
        val other = device(9, AudioDeviceInfo.TYPE_TELEPHONY)
        val again = refused.withDevices(Routing.ordered(refused.devices + other))!!
        assertEquals(sco, again.refused)
        assertEquals(sco, Routing.toAsk(again, Route.SPEAKER))
        // It leaves: no refusal said any more; the rule picks the speaker Kai is on already, so
        // nothing is asked.
        val gone = again.withDevices(Routing.ordered(listOf(earpiece, speaker, other)))!!
        assertNull(gone.refused)
        assertNull(gone.chosen)
        assertNull(Routing.toAsk(gone, Route.SPEAKER))
        assertEquals(speaker, gone.current)
    }
}
