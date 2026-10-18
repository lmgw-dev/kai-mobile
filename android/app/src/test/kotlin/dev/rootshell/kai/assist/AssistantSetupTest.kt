package dev.rootshell.kai.assist

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

/** Setup's assistant card (design §9.5): the open steps and the maker's button. */
class AssistantSetupTest {
    private fun facts(
        roleHeld: Boolean = true,
        microphone: Boolean = true,
        maker: Maker = Maker.SAMSUNG,
        power: Int? = null,
        ticked: Boolean = true,
    ) = AssistantFacts(roleAvailable = true, roleHeld = roleHeld, microphone = microphone, maker = maker, powerLongPress = power, buttonTicked = ticked)

    @Test
    fun theOpenStepsInTheCardsOrder() {
        assertEquals(emptyList<AssistantStep>(), AssistantSetup.open(facts()))
        assertEquals(
            listOf(AssistantStep.Role, AssistantStep.Button, AssistantStep.Microphone),
            AssistantSetup.open(facts(roleHeld = false, microphone = false, ticked = false)),
        )
    }

    @Test
    fun lineageOsPowerButtonIsReadWhereOtherMakersAreTicked() {
        // LineageOS: power_button_long_press 5 is the assistant (§9.4).
        assertTrue(AssistantSetup.buttonDone(facts(maker = Maker.LINEAGE, power = 5, ticked = false)))
        assertFalse(AssistantSetup.buttonDone(facts(maker = Maker.LINEAGE, power = 1, ticked = false)))
        assertFalse(AssistantSetup.buttonDone(facts(maker = Maker.LINEAGE, power = null, ticked = false)))
        // The same value means nothing on One UI, whose side button is its own setting.
        assertFalse(AssistantSetup.buttonDone(facts(maker = Maker.SAMSUNG, power = 5, ticked = false)))
        assertTrue(AssistantSetup.buttonDone(facts(maker = Maker.SAMSUNG, ticked = true)))
    }

    @Test
    fun theMakerComesFromTheBuildLineageOsFirst() {
        assertEquals(Maker.SAMSUNG, Maker.of("samsung", "example_product", "EXAMPLE.000"))
        // LineageOS on a phone a maker sold: by its product name, or by its display name.
        assertEquals(Maker.LINEAGE, Maker.of("samsung", "lineage_example", "EXAMPLE.000"))
        assertEquals(Maker.LINEAGE, Maker.of("Example", "example_product", "lineage_example-user 16"))
        assertEquals(Maker.PIXEL, Maker.of("Google", "example_product", "EXAMPLE.000"))
        assertEquals(Maker.OTHER, Maker.of("Example", "example_product", "EXAMPLE.000"))
    }

    @Test
    fun setKaiFollowsTheDesignsDefaultUntilWp0SaysOtherwise() {
        assertEquals(SetKai.VOICE_INPUT_SETTINGS, AssistantSetup.SET_KAI)
    }
}
