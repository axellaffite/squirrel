package app.squirrel.theme.components

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.RowScope
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.text.BasicText
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.semantics.LiveRegionMode
import androidx.compose.ui.semantics.ProgressBarRangeInfo
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.liveRegion
import androidx.compose.ui.semantics.progressBarRangeInfo
import androidx.compose.ui.semantics.semantics
import app.squirrel.theme.Shapes
import app.squirrel.theme.SquirrelIcon
import app.squirrel.theme.SquirrelTheme
import app.squirrel.theme.Tokens
import app.squirrel.theme.tabular

enum class AlertTone { Error, Success, Info }

/** MASTER.md "Alert": error on `--danger-soft`, success and information on `--tint`. */
@Composable
fun Alert(
    text: String,
    modifier: Modifier = Modifier,
    tone: AlertTone = AlertTone.Info,
    actions: (@Composable RowScope.() -> Unit)? = null,
) {
    val scheme = SquirrelTheme.scheme
    val (background, ink, icon) = when (tone) {
        AlertTone.Error -> Triple(scheme.dangerSoft, scheme.danger, SquirrelIcon.Alert)
        AlertTone.Success -> Triple(scheme.tint, scheme.success, SquirrelIcon.CheckCircle)
        AlertTone.Info -> Triple(scheme.tint, scheme.ink, SquirrelIcon.Info)
    }
    Column(
        modifier = modifier
            .fillMaxWidth()
            .background(background, Shapes.card)
            .padding(Tokens.cardPad)
            .semantics { liveRegion = LiveRegionMode.Polite },
        verticalArrangement = Arrangement.spacedBy(Tokens.space3),
    ) {
        Row(horizontalArrangement = Arrangement.spacedBy(Tokens.space2)) {
            Icon(icon, contentDescription = null, tint = ink)
            BasicText(text, style = SquirrelTheme.type.body.copy(color = ink))
        }
        if (actions != null) {
            Row(horizontalArrangement = Arrangement.spacedBy(Tokens.space2), content = actions)
        }
    }
}

/** A tinted pill with a 12 sp label in the tone colour. */
@Composable
fun Badge(text: String, modifier: Modifier = Modifier, tone: AlertTone = AlertTone.Info) {
    val scheme = SquirrelTheme.scheme
    val ink = when (tone) {
        AlertTone.Error -> scheme.danger
        AlertTone.Success -> scheme.success
        AlertTone.Info -> scheme.ink
    }
    BasicText(
        text,
        style = SquirrelTheme.type.badge.copy(color = ink).tabular(),
        modifier = modifier
            .background(scheme.tint, Shapes.pill)
            .padding(horizontal = Tokens.space2, vertical = Tokens.space1 / 2),
    )
}

/** MASTER.md "Progress": a 6 dp track pill with the accent fill; [label] names it. */
@Composable
fun Progress(fraction: Float, label: String, modifier: Modifier = Modifier) {
    val value = fraction.coerceIn(0f, 1f)
    val scheme = SquirrelTheme.scheme
    Box(
        modifier = modifier
            .fillMaxWidth()
            .height(Tokens.progressTrack)
            .background(scheme.track, Shapes.pill)
            .semantics {
                contentDescription = label
                progressBarRangeInfo = ProgressBarRangeInfo(value, 0f..1f)
            },
        contentAlignment = Alignment.CenterStart,
    ) {
        Box(Modifier.fillMaxWidth(value).fillMaxHeight().background(scheme.accent, Shapes.pill))
    }
}

/** The web's centred muted "Loading…": a first load that has nothing to show yet. */
@Composable
fun Loading(modifier: Modifier = Modifier, text: String = "Loading…") {
    Box(
        modifier = modifier
            .fillMaxWidth()
            .padding(Tokens.space6)
            .semantics { liveRegion = LiveRegionMode.Polite },
        contentAlignment = Alignment.Center,
    ) {
        BasicText(text, style = SquirrelTheme.type.caption)
    }
}
