package dev.rootshell.kai

import dev.rootshell.kai.ui.Layout
import dev.rootshell.kai.ui.layoutForWidthDp
import org.junit.Assert.assertEquals
import org.junit.Test

class AboutStateTest {
    @Test
    fun aboutShowsTheAppTheCoreAndThePin() {
        val about = AboutState.from("0.1.0+local-debug", FakeCore())
        assertEquals("0.1.0+local-debug", about.appVersion)
        assertEquals("0.1.0", about.coreVersion)
        assertEquals("release, aarch64-linux-android", about.coreBuild)
        assertEquals("0123456 (unpublished)", about.lmgwPin)
        assertEquals("fedcba987654", about.lmgwTree)
    }

    @Test
    fun theWideLayoutStartsAtSixHundredDp() {
        // A foldable's cover screen is narrower than 600 dp, its inner screen wider (design §10.6).
        assertEquals(Layout.Compact, layoutForWidthDp(412))
        assertEquals(Layout.Compact, layoutForWidthDp(599))
        assertEquals(Layout.Wide, layoutForWidthDp(600))
        assertEquals(Layout.Wide, layoutForWidthDp(884))
    }
}
