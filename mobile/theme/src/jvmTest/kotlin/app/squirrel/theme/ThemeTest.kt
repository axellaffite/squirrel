package app.squirrel.theme

import androidx.compose.ui.graphics.Color
import androidx.compose.ui.unit.dp
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertSame
import kotlin.test.assertTrue

class ThemeTest {
    @Test
    fun theSchemeFollowsTheSystemSetting() {
        assertSame(Tokens.Light, schemeFor(dark = false))
        assertSame(Tokens.Dark, schemeFor(dark = true))
    }

    @Test
    fun theAccentIsOrchidInBothSchemes() {
        assertEquals(Color(0xFF9A3FD0), Tokens.Light.accent)
        assertEquals(Color(0xFFD7A2FF), Tokens.Dark.accent)
    }

    @Test
    fun phoneValuesAreUsed() {
        assertEquals(112.dp, Tokens.photoRow)
        // The phone --shadow-card: 0 4px 12px, not the desktop 0 12px 32px.
        assertEquals(12.dp, Tokens.Light.shadowCard.last().blur)
        assertTrue(Tokens.Dark.shadowCard.first().inset)
    }

    @Test
    fun reducedMotionMakesEveryDurationZero() {
        val calm = Motion(reduced = true)
        assertEquals(listOf(0, 0, 0), listOf(calm.fastMillis, calm.baseMillis, calm.slowMillis))
        val normal = Motion(reduced = false)
        assertEquals(listOf(160, 240, 360), listOf(normal.fastMillis, normal.baseMillis, normal.slowMillis))
    }

    @Test
    fun theTypeScaleUsesTheTokens() {
        val type = squirrelType(androidx.compose.ui.text.font.FontFamily.Default, Tokens.Light)
        assertEquals(Tokens.textMd, type.body.fontSize)
        assertEquals(Tokens.weightStrong, type.strong.fontWeight)
        assertEquals("tnum", type.caption.tabular().fontFeatureSettings)
    }

    @Test
    fun touchTargetsAreAtLeast48dp() {
        assertTrue(app.squirrel.theme.components.MinTouchTarget >= 48.dp)
        assertTrue(Tokens.rowMin >= app.squirrel.theme.components.MinTouchTarget)
    }
}
