package app.squirrel.ui.explore

import androidx.compose.foundation.layout.ExperimentalLayoutApi
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.text.BasicText
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.style.TextOverflow
import app.squirrel.api.models.PersonDto
import app.squirrel.shared.api.SquirrelApi
import app.squirrel.theme.Shapes
import app.squirrel.theme.SquirrelIcon
import app.squirrel.theme.SquirrelTheme
import app.squirrel.theme.Tokens
import app.squirrel.theme.boxShadow
import app.squirrel.theme.components.Button
import app.squirrel.theme.components.ButtonSize
import app.squirrel.theme.components.ButtonVariant
import app.squirrel.theme.components.Icon

/**
 * The "Filter" button, then one chip per part of [filter] (MASTER "Filter chip": active chips
 * are chosen segments with a clear button), then "Clear filters".
 */
@OptIn(ExperimentalLayoutApi::class)
@Composable
fun FilterBar(api: SquirrelApi, filter: PhotoFilter, people: List<PersonDto>, onChange: (PhotoFilter) -> Unit) {
    var picking by remember { mutableStateOf(false) }
    val person = filter.person?.let { id -> people.firstOrNull { it.id == id } }
    val list = chips(filter) { id -> people.firstOrNull { it.id == id }?.name?.trim()?.takeIf { it.isNotEmpty() } }
    FlowRow(
        horizontalArrangement = Arrangement.spacedBy(Tokens.space2),
        verticalArrangement = Arrangement.spacedBy(Tokens.space1),
        itemVerticalAlignment = Alignment.CenterVertically,
    ) {
        Button("Filter", onClick = { picking = true }, variant = ButtonVariant.Secondary, size = ButtonSize.Sm, icon = SquirrelIcon.SlidersHorizontal)
        list.forEach { chip -> ActiveChip(api, chip, person, onRemove = { onChange(chip.without) }) }
        if (list.isNotEmpty()) Button("Clear filters", onClick = { onChange(PhotoFilter()) }, variant = ButtonVariant.Quiet, size = ButtonSize.Sm)
    }
    if (picking) FilterPicker(api, filter, people, onPick = onChange, onClose = { picking = false })
}

@Composable
private fun ActiveChip(api: SquirrelApi, chip: Chip, person: PersonDto?, onRemove: () -> Unit) {
    val scheme = SquirrelTheme.scheme
    Box(contentAlignment = Alignment.Center) {
        Row(
            modifier = Modifier
                .heightIn(min = Tokens.chipH)
                .boxShadow(scheme.shadowKnob, Shapes.pill)
                .background(scheme.card, Shapes.pill)
                .padding(start = Tokens.space3),
            horizontalArrangement = Arrangement.spacedBy(Tokens.space1),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            if (chip.kind == ChipKind.Person) {
                FaceAvatar(api, person?.cover, size = Tokens.iconMd)
            } else {
                Icon(chip.kind.icon, contentDescription = null, tint = scheme.inkMuted)
            }
            BasicText(
                chip.label,
                style = SquirrelTheme.type.label,
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
                modifier = Modifier.semantics { contentDescription = "Filter ${chip.label}" },
            )
            ClearButton("Remove filter ${chip.label}", onRemove)
        }
    }
}

/**
 * The chip's trailing `--chip-clear` `x`, drawn inside the 36 dp chip; Compose widens its touch
 * bounds to the 48 dp minimum.
 */
@Composable
private fun ClearButton(label: String, onClick: () -> Unit) {
    Box(
        modifier = Modifier
            .size(Tokens.chipH)
            .clip(Shapes.circle)
            .clickable(role = Role.Button, onClick = onClick)
            .semantics { contentDescription = label },
        contentAlignment = Alignment.Center,
    ) {
        Icon(SquirrelIcon.X, contentDescription = null, tint = SquirrelTheme.scheme.inkMuted, size = Tokens.chipClear)
    }
}
