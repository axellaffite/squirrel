package app.squirrel.theme

import androidx.compose.runtime.Composable
import androidx.compose.runtime.Immutable
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontVariation
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.LineHeightStyle
import androidx.compose.ui.unit.TextUnit
import app.squirrel.theme.resources.Res
import app.squirrel.theme.resources.geist
import org.jetbrains.compose.resources.Font

/** The type scale of MASTER.md "Typography", phone sizes. */
@Immutable
class SquirrelType(
    /** `--text-md` 400: body, inputs, row metadata on cards. */
    val body: TextStyle,
    /** `--text-md` 500: row names, field values, secondary buttons. */
    val label: TextStyle,
    /** `--text-md` 600: primary buttons. */
    val strong: TextStyle,
    /** `--text-sm` 400 muted: metadata, captions, hints. */
    val caption: TextStyle,
    /** `--text-sm` 500: field labels. */
    val fieldLabel: TextStyle,
    /** `--text-xs` 500: badges and rail labels. */
    val badge: TextStyle,
    /** `--text-lg` 500: card titles, empty-state titles. */
    val cardTitle: TextStyle,
    /** `--text-xl` 500: section headings, dialog titles. */
    val sectionTitle: TextStyle,
    /** `--text-xl` 600: the wordmark. */
    val wordmark: TextStyle,
    /** `--text-2xl` 500: page titles on phones. */
    val pageTitle: TextStyle,
    /** `--text-display`: the home greeting only. */
    val display: TextStyle,
)

/** MASTER.md: tabular figures on every size, count, date and progress figure. */
fun TextStyle.tabular(): TextStyle = copy(fontFeatureSettings = "tnum")

/** Geist (variable), the same font as the web, at the three weights MASTER.md uses. */
@Composable
fun geistFamily(): FontFamily = FontFamily(
    listOf(Tokens.weightBody, Tokens.weightLabel, Tokens.weightStrong).map { weight ->
        Font(
            resource = Res.font.geist,
            weight = weight,
            variationSettings = FontVariation.Settings(FontVariation.weight(weight.weight)),
        )
    },
)

private fun style(
    family: FontFamily,
    size: TextUnit,
    lineHeight: TextUnit,
    weight: FontWeight,
    tracking: TextUnit = TextUnit.Unspecified,
) = TextStyle(
    fontFamily = family,
    fontSize = size,
    lineHeight = lineHeight,
    fontWeight = weight,
    letterSpacing = tracking,
    lineHeightStyle = LineHeightStyle(
        alignment = LineHeightStyle.Alignment.Center,
        trim = LineHeightStyle.Trim.None,
    ),
)

/** Builds the scale; colours are left to the components (ink or muted per use). */
fun squirrelType(family: FontFamily, scheme: SchemeTokens): SquirrelType = with(Tokens) {
    SquirrelType(
        body = style(family, textMd, textMdLh, weightBody).copy(color = scheme.ink),
        label = style(family, textMd, textMdLh, weightLabel, trackLabel).copy(color = scheme.ink),
        strong = style(family, textMd, textMdLh, weightStrong, trackLabel).copy(color = scheme.ink),
        caption = style(family, textSm, textSmLh, weightBody).copy(color = scheme.inkMuted),
        fieldLabel = style(family, textSm, textSmLh, weightLabel, trackLabel).copy(color = scheme.ink),
        badge = style(family, textXs, textXsLh, weightLabel).copy(color = scheme.ink),
        cardTitle = style(family, textLg, textLgLh, weightHeading, trackHeading).copy(color = scheme.ink),
        sectionTitle = style(family, textXl, textXlLh, weightHeading, trackHeading).copy(color = scheme.ink),
        wordmark = style(family, textXl, textXlLh, weightStrong, trackHeading).copy(color = scheme.ink),
        pageTitle = style(family, text2xl, text2xlLh, weightHeading, trackTight).copy(color = scheme.ink),
        display = style(family, textDisplay, textDisplayLh, weightHeading, trackTight).copy(color = scheme.ink),
    )
}
