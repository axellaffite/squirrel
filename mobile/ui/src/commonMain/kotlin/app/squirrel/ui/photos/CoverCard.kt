package app.squirrel.ui.photos

import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.aspectRatio
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.text.BasicText
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.text.style.TextOverflow
import app.squirrel.api.models.AssetSummary
import app.squirrel.api.models.ThumbnailStatus
import app.squirrel.shared.api.SquirrelApi
import app.squirrel.theme.Shapes
import app.squirrel.theme.SquirrelIcon
import app.squirrel.theme.SquirrelTheme
import app.squirrel.theme.Tokens
import app.squirrel.theme.cardShadow
import app.squirrel.theme.components.Icon
import app.squirrel.theme.tabular
import app.squirrel.ui.images.Thumbnail

/**
 * MASTER "Album card", also for tags and places: a white card holding a 4:3 cover
 * (`--radius-tile`; without a cover, `--tint` with [icon]), the name 500 and the count.
 */
@Composable
fun CoverCard(
    api: SquirrelApi,
    cover: AssetSummary?,
    name: String,
    count: String,
    onClick: () -> Unit,
    modifier: Modifier = Modifier,
    icon: SquirrelIcon = SquirrelIcon.Library,
    extra: (@Composable () -> Unit)? = null,
) {
    val scheme = SquirrelTheme.scheme
    Column(
        modifier = modifier
            .fillMaxWidth()
            .cardShadow(Shapes.card)
            .clip(Shapes.card)
            .background(scheme.card)
            .clickable(role = Role.Button, onClickLabel = "Open", onClick = onClick)
            .padding(Tokens.space2),
        verticalArrangement = Arrangement.spacedBy(Tokens.space1),
    ) {
        val coverModifier = Modifier.fillMaxWidth().aspectRatio(Tokens.albumRatio).clip(Shapes.tile)
        if (cover == null) {
            Box(coverModifier.background(scheme.tint), contentAlignment = Alignment.Center) {
                Icon(icon, contentDescription = null, tint = scheme.inkMuted)
            }
        } else {
            Thumbnail(
                url = cover.takeIf { it.thumbnail.state == ThumbnailStatus.ready }?.let { api.thumbnailUrl(it.id, it.thumbnail.version) },
                blurhash = cover.blurhash,
                contentDescription = null,
                modifier = coverModifier,
            )
        }
        BasicText(name, style = SquirrelTheme.type.label, maxLines = 1, overflow = TextOverflow.Ellipsis)
        BasicText(count, style = SquirrelTheme.type.caption.tabular(), maxLines = 1, overflow = TextOverflow.Ellipsis)
        extra?.invoke()
    }
}

/** [items] two to a row (album, tag and place cards on phones). */
@Composable
fun <T> TwoColumns(items: List<T>, card: @Composable (T, Modifier) -> Unit) {
    Column(verticalArrangement = Arrangement.spacedBy(Tokens.space3)) {
        items.chunked(2).forEach { pair ->
            androidx.compose.foundation.layout.Row(horizontalArrangement = Arrangement.spacedBy(Tokens.space3)) {
                pair.forEach { card(it, Modifier.weight(1f)) }
                if (pair.size == 1) Box(Modifier.weight(1f))
            }
        }
    }
}
