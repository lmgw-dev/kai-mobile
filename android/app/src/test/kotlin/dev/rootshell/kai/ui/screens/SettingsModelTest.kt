package dev.rootshell.kai.ui.screens

import dev.rootshell.kai.R
import java.io.File
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

/** The settings pages' rules (design §3.4, 2026-10-19): the conversation choice, the numbers, the labels. */
class SettingsModelTest {
    @Test
    fun theSharedNameIsTheSharedChoiceAndAnyOtherNameIsThisPhones() {
        assertEquals(ConversationChoice.SHARED, ConversationName.choiceOf("Kai"))
        assertEquals(ConversationChoice.THIS_PHONE, ConversationName.choiceOf("Kai mobile"))
        assertEquals(ConversationChoice.THIS_PHONE, ConversationName.choiceOf("Work"))
        // Exactly "Kai": a name that differs in case is a folder of its own.
        assertEquals(ConversationChoice.THIS_PHONE, ConversationName.choiceOf("kai"))
    }

    @Test
    fun theNameFieldStartsWithTheStoredOwnNameElseTheDefault() {
        assertEquals("Kai Pixel 9", ConversationName.fieldStart("Kai", "Pixel 9"))
        assertEquals("Work", ConversationName.fieldStart("Work", "Pixel 9"))
    }

    @Test
    fun theDefaultNameCarriesTheDeviceNameSoTwoPhonesDiffer() {
        assertEquals("Kai Anna's phone", ConversationName.thisPhoneDefault("  Anna's phone "))
        assertEquals("Kai mobile", ConversationName.thisPhoneDefault("  "))
        assertEquals(ConversationName.THIS_PHONE_DEFAULT, ConversationName.thisPhoneDefault(""))
        // The default is itself a valid own name.
        assertNull(ConversationName.refusal(ConversationChoice.THIS_PHONE, ConversationName.thisPhoneDefault("Pixel 9")))
    }

    @Test
    fun aChoiceSavesItsFolderName() {
        assertEquals("Kai", ConversationName.folderFor(ConversationChoice.SHARED, "whatever was typed"))
        assertEquals("Kai", ConversationName.folderFor(ConversationChoice.SHARED, ""))
        assertEquals("Kai mobile", ConversationName.folderFor(ConversationChoice.THIS_PHONE, "  Kai mobile "))
    }

    @Test
    fun anOwnNameMustBeThereAndMustNotBeTheSharedOne() {
        assertEquals(NameRefusal.EMPTY, ConversationName.refusal(ConversationChoice.THIS_PHONE, "  "))
        assertEquals(NameRefusal.IS_SHARED, ConversationName.refusal(ConversationChoice.THIS_PHONE, " Kai"))
        assertNull(ConversationName.refusal(ConversationChoice.THIS_PHONE, "Kai mobile"))
        assertNull(ConversationName.folderFor(ConversationChoice.THIS_PHONE, "Kai"))
        // The shared choice ignores the field.
        assertNull(ConversationName.refusal(ConversationChoice.SHARED, ""))
    }

    @Test
    fun theChoiceRoundTrips() {
        for (stored in listOf("Kai", "Kai mobile", "Work")) {
            val choice = ConversationName.choiceOf(stored)
            assertEquals(stored, ConversationName.folderFor(choice, ConversationName.fieldStart(stored, "Pixel 9")))
        }
    }

    @Test
    fun wholeNumbersAreTrimmedAndAnOptionalOneMayBeEmpty() {
        assertTrue(WholeNumber.valid(" 30 "))
        assertFalse(WholeNumber.valid(""))
        assertFalse(WholeNumber.valid("1.5"))
        assertFalse(WholeNumber.valid("-1"))
        assertTrue(WholeNumber.valid("", optional = true))
        assertFalse(WholeNumber.valid("x", optional = true))
        assertEquals(30u, WholeNumber.parse(" 30 "))
        assertNull(WholeNumber.parseOptional("  "))
        assertEquals(300u, WholeNumber.parseOptional("300"))
    }

    @Test
    fun everySettingTheCoreCanRefuseHasALabel() {
        // The keys Settings::validate names in the core, and the voice fields the page parses.
        val keys = listOf(
            "lmgw.timeout_s", "lmgw.retry_max_s", "lmgw.retry_max_background_s", "feed.keepalive_s",
            "pairing.confirm_poll_s", "voice.connect_timeout_s", "voice.playout_report_ms",
            "conversation.folder_name",
        )
        for (key in keys) {
            assertNotNull(key, SettingKey.of(key))
            assertTrue(key, Refusals.of(key).labels.isNotEmpty())
        }
    }

    @Test
    fun aRefusalNamesTheLabelsAndNotTheKeys() {
        assertEquals(RefusalWords(R.string.set_refused_below, listOf(R.string.set_retry_background, R.string.set_retry)), Refusals.of("lmgw.retry_max_background_s"))
        assertEquals(RefusalWords(R.string.set_refused_empty, listOf(R.string.set_conversation_name)), Refusals.of("conversation.folder_name"))
        assertEquals(RefusalWords(R.string.set_refused_at_least_one, listOf(R.string.set_keepalive)), Refusals.of("feed.keepalive_s"))
        assertEquals(RefusalWords(R.string.set_refused_unknown, emptyList()), Refusals.of("no.such_key"))
    }

    @Test
    fun noScreenShowsAConfigKey() {
        // The owner's rule (2026-10-19): a config key is never a label. Only the model may spell the keys.
        val key = Regex("\"(lmgw|feed|pairing|conversation|voice|appearance)\\.[a-z]+(_[a-z]+)*\"")
        val src = File("src/main/kotlin")
        val offenders = src.walkTopDown().filter { it.isFile && it.extension == "kt" && it.name != "SettingsModel.kt" }
            .flatMap { f -> f.readLines().withIndex().filter { key.containsMatchIn(it.value) }.map { "${f.name}:${it.index + 1}" } }
            .toList()
        assertTrue(offenders.toString(), offenders.isEmpty())
        val strings = File("src/main/res/values/strings.xml").readText()
        assertFalse(Regex("\\b(lmgw|feed|pairing|conversation|voice)\\.[a-z]+_[a-z_]+\\b").containsMatchIn(strings))
        assertFalse(Regex("\\b[a-z]+_[a-z]+_(s|ms)\\b").containsMatchIn(strings))
    }
}
