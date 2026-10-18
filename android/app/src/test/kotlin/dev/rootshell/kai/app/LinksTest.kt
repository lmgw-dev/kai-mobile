package dev.rootshell.kai.app

import dev.rootshell.kai.core.ffi.NotificationId
import dev.rootshell.kai.core.ffi.NotificationKind
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotEquals
import org.junit.Assert.assertNull
import org.junit.Test

class LinksTest {
    @Test
    fun notificationTagsRoundTripAndSayWhatIsActive() {
        val ids = listOf(
            NotificationId(NotificationKind.APPROVAL, "mcpr_1"),
            NotificationId(NotificationKind.REPLY, "7"),
            NotificationId(NotificationKind.NOTICE, "core"),
        )
        ids.forEach { assertEquals(it, NotificationTags.parse(NotificationTags.tag(it))) }
        assertNull(NotificationTags.parse("other"))
        assertNull(NotificationTags.parse(null))
        val (approvals, replies) = NotificationTags.active(listOf("approval:mcpr_1", "reply:7", "reply:x", null, "notice:core"))
        assertEquals(listOf("mcpr_1"), approvals)
        assertEquals(listOf(7L), replies)
    }

    @Test
    fun eachNoticeHasATagOfItsOwnAndTheSameSentenceReplacesItself() {
        val a = NotificationTags.noticeKey("lmgw did not take the decision")
        assertEquals(a, NotificationTags.noticeKey("lmgw did not take the decision"))
        assertNotEquals(a, NotificationTags.noticeKey("The pairing could not be saved"))
        assertEquals(16, a.length)
        val id = NotificationId(NotificationKind.NOTICE, a)
        assertEquals(id, NotificationTags.parse(NotificationTags.tag(id)))
    }

    @Test
    fun onlyAPairingLinkIsHandedOn() {
        assertEquals("lmgw-pair:?v=2&name=p", PairingLinks.of("  lmgw-pair:?v=2&name=p\n"))
        assertEquals("LMGW-PAIR:?v=2", PairingLinks.of("LMGW-PAIR:?v=2"))
        assertNull(PairingLinks.of("https://example.com"))
        assertNull(PairingLinks.of(null))
    }

    @Test
    fun theBackgroundCardSaysWhatIsOpen() {
        val all = BackgroundFacts(notificationsAllowed = false, batteryExempt = false, samsung = true, neverSleepingTicked = false)
        assertEquals(listOf(BackgroundStep.Notifications, BackgroundStep.Battery, BackgroundStep.NeverSleeping), BackgroundSetup.open(all))
        assertEquals(emptyList<BackgroundStep>(), BackgroundSetup.open(all.copy(notificationsAllowed = true, batteryExempt = true, samsung = false)))
        assertEquals(emptyList<BackgroundStep>(), BackgroundSetup.open(BackgroundFacts(true, true, true, true)))
    }
}
