// Generated from web/src/lib/styles/tokens.css by web/scripts/native-tokens.mjs. Do not edit.
@file:Suppress("MagicNumber")

package app.squirrel.theme

import androidx.compose.animation.core.CubicBezierEasing
import androidx.compose.runtime.Immutable
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.em
import androidx.compose.ui.unit.sp

/** The colours, gradients and shadows of one scheme (light or dark). */
@Immutable
class SchemeTokens(
    val ground: GradientToken,
    val groundSolid: Color,
    val panel: GradientToken,
    val panelSolid: Color,
    val card: Color,
    val tint: Color,
    val tintStrong: Color,
    val hair: Color,
    val ink: Color,
    val inkMuted: Color,
    val edge: Color,
    val accent: Color,
    val accentInk: Color,
    val onAccent: Color,
    val danger: Color,
    val onDanger: Color,
    val success: Color,
    val warning: Color,
    val rail: GradientToken,
    val railInk: Color,
    val railMuted: Color,
    val railWell: Color,
    val railWellHover: Color,
    val railActive: Color,
    val railActiveInk: Color,
    val track: Color,
    val fill: Color,
    val accentSoft: Color,
    val dangerSoft: Color,
    val scrim: Color,
    val shadowCard: List<ShadowLayer>,
    val shadowHover: List<ShadowLayer>,
    val shadowPop: List<ShadowLayer>,
    val shadowKnob: List<ShadowLayer>,
    val shadowActive: List<ShadowLayer>,
    val panelEdge: List<ShadowLayer>,
    val glow: List<ShadowLayer>,
    val glowHover: List<ShadowLayer>,
    val selectRing: List<ShadowLayer>,
    val photoRing: Color,
    val photoShade: Color,
)

/** Every token of tokens.css that has a native meaning, at its phone value. */
object Tokens {
    val ringWidth = 2.dp
    val ringOffset = 2.dp
    val textXs = 12.sp
    val textXsLh = 16.sp
    val textSm = 13.sp
    val textSmLh = 18.sp
    val textMd = 14.sp
    val textMdLh = 20.sp
    val textLg = 16.sp
    val textLgLh = 22.sp
    val textXl = 19.sp
    val textXlLh = 24.sp
    val text2xl = 24.sp
    val text2xlLh = 28.sp
    val text3xl = 30.sp
    val text3xlLh = 34.sp
    val textDisplay = 28.sp
    val textDisplayLh = 1.05.em
    val weightBody = FontWeight(400)
    val weightLabel = FontWeight(500)
    val weightStrong = FontWeight(600)
    val weightHeading = FontWeight(500)
    val trackTight = (-0.035).em
    val trackHeading = (-0.02).em
    val trackLabel = (-0.01).em
    val trackCaps = 0.04.em
    val space1 = 4.dp
    val space2 = 8.dp
    val space3 = 12.dp
    val space4 = 16.dp
    val space5 = 24.dp
    val space6 = 32.dp
    val space7 = 48.dp
    val space8 = 64.dp
    val gutter = 16.dp
    val frame = 12.dp
    val panelPad = 18.dp
    val cardPad = 14.dp
    val wellPad = 16.dp
    val contentMax = 1120.dp
    val tap = 44.dp
    val radiusSm = 12.dp
    val radiusTile = 6.dp
    val radiusCard = 18.dp
    val radiusWell = 22.dp
    val radiusPanel = 26.dp
    val radiusRail = 28.dp
    val radiusFull = 999.dp
    val iconSm = 20.dp
    val iconMd = 24.dp
    val controlSm = 36.dp
    val controlMd = 44.dp
    val controlLg = 48.dp
    val wellSm = 36.dp
    val wellMd = 40.dp
    val wellLg = 56.dp
    val fab = 44.dp
    val rowMin = 54.dp
    val rowNameMin = 128.dp
    val quickMin = 56.dp
    val meterTrack = 38.dp
    val meterPad = 6.dp
    val progressTrack = 6.dp
    val railW = 210.dp
    val railBarH = 72.dp
    val press = 1.dp
    const val disabledOpacity = 0.45f
    val authMax = 400.dp
    val dialogMax = 480.dp
    val menuW = 272.dp
    val nameMax = 160.dp
    val underlineOffset = 0.2.em
    val photoRow = 112.dp
    val photoMonthHead = 48.dp
    val photoDayHead = 32.dp
    val selectCheck = 24.dp
    val albumMin = 160.dp
    const val albumRatio = 1.3333f
    val pickerMax = 352.dp
    val scrubberW = 56.dp
    val scrubTick = 2.dp
    val infoW = 360.dp
    val locatorDot = 6.dp
    val chipH = 36.dp
    val chipClear = 20.dp
    val faceSm = 40.dp
    val faceMd = 64.dp
    val faceLg = 112.dp
    val personMin = 112.dp
    val faceBox = 2.dp
    const val durFast = 160
    const val dur = 240
    const val durSlow = 360
    val ease = CubicBezierEasing(0.22f, 1f, 0.36f, 1f)
    val rise = 8.dp
    val riseSheet = 16.dp
    const val stagger = 30

    val Light = SchemeTokens(
        ground = GradientToken(top = Color(0xFFE4E4E7), bottom = Color(0xFFCFCFD3)),
        groundSolid = Color(0xFFDADADD),
        panel = GradientToken(top = Color(0xFFF6F6F7), bottom = Color(0xFFECECEE)),
        panelSolid = Color(0xFFF1F1F2),
        card = Color(0xFFFFFFFF),
        tint = Color(red = 20, green = 20, blue = 30, alpha = 11),
        tintStrong = Color(red = 20, green = 20, blue = 30, alpha = 22),
        hair = Color(red = 20, green = 20, blue = 30, alpha = 20),
        ink = Color(0xFF111214),
        inkMuted = Color(0xFF5C5E65),
        edge = Color(0xFF7F8189),
        accent = Color(0xFF9A3FD0),
        accentInk = Color(0xFF8A3AC0),
        onAccent = Color(0xFFFFFFFF),
        danger = Color(0xFFB3261E),
        onDanger = Color(0xFFFFFFFF),
        success = Color(0xFF1D6B44),
        warning = Color(0xFF7A4F00),
        rail = GradientToken(top = Color(0xFF4C4D52), bottom = Color(0xFF2A2B2F)),
        railInk = Color(0xFFF4F4F5),
        railMuted = Color(0xFFC8C9CD),
        railWell = Color(red = 255, green = 255, blue = 255, alpha = 20),
        railWellHover = Color(red = 255, green = 255, blue = 255, alpha = 41),
        railActive = Color(0xFFFFFFFF),
        railActiveInk = Color(0xFF8A3AC0),
        track = Color(red = 20, green = 20, blue = 30, alpha = 18),
        fill = Color(0xFFFFFFFF),
        accentSoft = Color(0xFFF3E8F9),
        dangerSoft = Color(0xFFF8EBEB),
        scrim = Color(red = 17, green = 18, blue = 20, alpha = 107),
        shadowCard = listOf(ShadowLayer(x = 0.dp, y = 1.dp, blur = 2.dp, spread = 0.dp, color = Color(red = 20, green = 20, blue = 30, alpha = 13), inset = false), ShadowLayer(x = 0.dp, y = 4.dp, blur = 12.dp, spread = 0.dp, color = Color(red = 20, green = 20, blue = 30, alpha = 18), inset = false)),
        shadowHover = listOf(ShadowLayer(x = 0.dp, y = 1.dp, blur = 2.dp, spread = 0.dp, color = Color(red = 20, green = 20, blue = 30, alpha = 13), inset = false), ShadowLayer(x = 0.dp, y = 4.dp, blur = 12.dp, spread = 0.dp, color = Color(red = 20, green = 20, blue = 30, alpha = 18), inset = false)),
        shadowPop = listOf(ShadowLayer(x = 0.dp, y = 2.dp, blur = 6.dp, spread = 0.dp, color = Color(red = 20, green = 20, blue = 30, alpha = 20), inset = false), ShadowLayer(x = 0.dp, y = 10.dp, blur = 24.dp, spread = 0.dp, color = Color(red = 20, green = 20, blue = 30, alpha = 41), inset = false)),
        shadowKnob = listOf(ShadowLayer(x = 0.dp, y = 2.dp, blur = 8.dp, spread = 0.dp, color = Color(red = 0, green = 0, blue = 0, alpha = 20), inset = false)),
        shadowActive = listOf(ShadowLayer(x = 0.dp, y = 4.dp, blur = 14.dp, spread = 0.dp, color = Color(red = 0, green = 0, blue = 0, alpha = 64), inset = false)),
        panelEdge = listOf(ShadowLayer(x = 0.dp, y = 1.dp, blur = 0.dp, spread = 0.dp, color = Color(red = 255, green = 255, blue = 255, alpha = 153), inset = true)),
        glow = listOf(ShadowLayer(x = 0.dp, y = 8.dp, blur = 22.dp, spread = 0.dp, color = Color(red = 154, green = 63, blue = 208, alpha = 115), inset = false)),
        glowHover = listOf(ShadowLayer(x = 0.dp, y = 10.dp, blur = 28.dp, spread = 0.dp, color = Color(red = 154, green = 63, blue = 208, alpha = 140), inset = false)),
        selectRing = listOf(ShadowLayer(x = 0.dp, y = 0.dp, blur = 0.dp, spread = 2.dp, color = Color(0xFF9A3FD0), inset = false)),
        photoRing = Color(red = 255, green = 255, blue = 255, alpha = 179),
        photoShade = Color(red = 0, green = 0, blue = 0, alpha = 64),
    )

    val Dark = SchemeTokens(
        ground = GradientToken(top = Color(0xFF17181B), bottom = Color(0xFF0B0B0D)),
        groundSolid = Color(0xFF111214),
        panel = GradientToken(top = Color(0xFF1D1E21), bottom = Color(0xFF18191B)),
        panelSolid = Color(0xFF1B1C1F),
        card = Color(0xFF26272B),
        tint = Color(red = 255, green = 255, blue = 255, alpha = 11),
        tintStrong = Color(red = 255, green = 255, blue = 255, alpha = 22),
        hair = Color(red = 255, green = 255, blue = 255, alpha = 18),
        ink = Color(0xFFF1F1F2),
        inkMuted = Color(0xFF9FA1A8),
        edge = Color(0xFF80838B),
        accent = Color(0xFFD7A2FF),
        accentInk = Color(0xFFD7A2FF),
        onAccent = Color(0xFF1F0B2E),
        danger = Color(0xFFFF9A8F),
        onDanger = Color(0xFF26272B),
        success = Color(0xFF74D3A3),
        warning = Color(0xFFF0C062),
        rail = GradientToken(top = Color(0xFF303136), bottom = Color(0xFF1D1E21)),
        railInk = Color(0xFFF1F1F2),
        railMuted = Color(0xFFA3A5AB),
        railWell = Color(red = 255, green = 255, blue = 255, alpha = 18),
        railWellHover = Color(red = 255, green = 255, blue = 255, alpha = 36),
        railActive = Color(0xFFF1F1F2),
        railActiveInk = Color(0xFF8A3AC0),
        track = Color(red = 255, green = 255, blue = 255, alpha = 18),
        fill = Color(0xFFE9E9EC),
        accentSoft = Color(0xFF3B3644),
        dangerSoft = Color(0xFF3A3134),
        scrim = Color(red = 0, green = 0, blue = 0, alpha = 153),
        shadowCard = listOf(ShadowLayer(x = 0.dp, y = 1.dp, blur = 0.dp, spread = 0.dp, color = Color(red = 255, green = 255, blue = 255, alpha = 10), inset = true), ShadowLayer(x = 0.dp, y = 4.dp, blur = 12.dp, spread = 0.dp, color = Color(red = 0, green = 0, blue = 0, alpha = 102), inset = false)),
        shadowHover = listOf(ShadowLayer(x = 0.dp, y = 1.dp, blur = 0.dp, spread = 0.dp, color = Color(red = 255, green = 255, blue = 255, alpha = 10), inset = true), ShadowLayer(x = 0.dp, y = 4.dp, blur = 12.dp, spread = 0.dp, color = Color(red = 0, green = 0, blue = 0, alpha = 102), inset = false)),
        shadowPop = listOf(ShadowLayer(x = 0.dp, y = 1.dp, blur = 0.dp, spread = 0.dp, color = Color(red = 255, green = 255, blue = 255, alpha = 13), inset = true), ShadowLayer(x = 0.dp, y = 10.dp, blur = 24.dp, spread = 0.dp, color = Color(red = 0, green = 0, blue = 0, alpha = 140), inset = false)),
        shadowKnob = listOf(ShadowLayer(x = 0.dp, y = 2.dp, blur = 8.dp, spread = 0.dp, color = Color(red = 0, green = 0, blue = 0, alpha = 89), inset = false)),
        shadowActive = listOf(ShadowLayer(x = 0.dp, y = 4.dp, blur = 14.dp, spread = 0.dp, color = Color(red = 0, green = 0, blue = 0, alpha = 115), inset = false)),
        panelEdge = listOf(ShadowLayer(x = 0.dp, y = 1.dp, blur = 0.dp, spread = 0.dp, color = Color(red = 255, green = 255, blue = 255, alpha = 10), inset = true)),
        glow = listOf(ShadowLayer(x = 0.dp, y = 8.dp, blur = 22.dp, spread = 0.dp, color = Color(red = 215, green = 162, blue = 255, alpha = 115), inset = false)),
        glowHover = listOf(ShadowLayer(x = 0.dp, y = 10.dp, blur = 28.dp, spread = 0.dp, color = Color(red = 215, green = 162, blue = 255, alpha = 140), inset = false)),
        selectRing = listOf(ShadowLayer(x = 0.dp, y = 0.dp, blur = 0.dp, spread = 2.dp, color = Color(0xFFD7A2FF), inset = false)),
        photoRing = Color(red = 255, green = 255, blue = 255, alpha = 179),
        photoShade = Color(red = 0, green = 0, blue = 0, alpha = 64),
    )
}
