package app.squirrel.ui.photos

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.aspectRatio
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.text.BasicText
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.foundation.verticalScroll
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.semantics.heading
import androidx.compose.ui.semantics.liveRegion
import androidx.compose.ui.semantics.LiveRegionMode
import androidx.compose.ui.semantics.semantics
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import app.squirrel.api.models.AlbumDto
import app.squirrel.api.models.AssetSummary
import app.squirrel.api.models.MatchMode
import app.squirrel.api.models.PhotoKindFilter
import app.squirrel.api.models.ThumbnailStatus
import app.squirrel.shared.api.SquirrelApi
import app.squirrel.theme.Shapes
import app.squirrel.theme.SquirrelIcon
import app.squirrel.theme.SquirrelTheme
import app.squirrel.theme.Tokens
import app.squirrel.theme.components.Alert
import app.squirrel.theme.components.AlertTone
import app.squirrel.theme.components.Button
import app.squirrel.theme.components.Card
import app.squirrel.theme.components.ListRow
import app.squirrel.theme.components.RowGroup
import app.squirrel.theme.components.SegmentedControl
import app.squirrel.theme.components.TextField
import app.squirrel.theme.tabular
import app.squirrel.ui.explore.countryName
import app.squirrel.ui.explore.personLabel
import app.squirrel.ui.explore.photoCount
import app.squirrel.ui.explore.tagLabel
import app.squirrel.ui.images.Thumbnail
import app.squirrel.ui.shell.PageHeader
import app.squirrel.ui.shell.SystemBack
import io.github.axellaffite.lsdo.compose.RegisterLsDo
import kotlinx.coroutines.launch

private enum class RuleSheet { People, Tags, Place, Camera }

private val KINDS = listOf(null to "Any", PhotoKindFilter.photo to "Photos only", PhotoKindFilter.screenshot to "Screenshots")

/** Thumbnails of the newest matching photos the preview shows. */
private const val PREVIEW_TILES = 6

/**
 * The rule editor of a dynamic album: its name (when new), the rule groups (people, tags, place,
 * camera, kind, dates), and a live count of the photos they match. [onSaved] gets the album.
 */
@Composable
fun AlbumRulesPage(api: SquirrelApi, vm: AlbumRulesViewModel, title: String, onSaved: (AlbumDto) -> Unit, onBack: () -> Unit) {
    val state by vm.state.collectAsStateWithLifecycle()
    var sheet by remember { mutableStateOf<RuleSheet?>(null) }
    val scope = rememberCoroutineScope()
    val save: () -> Unit = { scope.launch { vm.save()?.let(onSaved) } }
    RegisterLsDo("album_rules", remember(vm) { AlbumRulesLsDo(vm, onSaved, onBack) })
    SystemBack(onBack = onBack)
    val form = state.form
    Column(Modifier.fillMaxSize(), verticalArrangement = Arrangement.spacedBy(Tokens.space2)) {
        PageHeader(title, onBack = onBack, backLabel = "Cancel")
        Column(
            Modifier.fillMaxSize().verticalScroll(rememberScrollState()),
            verticalArrangement = Arrangement.spacedBy(Tokens.space3),
        ) {
            if (state.creating) TextField(state.name, vm::setName, label = "Name")
            BasicText("Photos that match every rule below show up in the album by themselves, now and later.", style = SquirrelTheme.type.caption)
            PreviewCard(api, state)
            if (state.choicesFailed) Alert(UNREACHABLE, tone = AlertTone.Error)
            RowGroup {
                val names = state.people.associate { it.id to it.name }
                ListRow("People", icon = SquirrelIcon.User, meta = groupMeta(form.people.map { personLabel(names[it]) }, form.peopleMatch, "Anyone"), onClick = { sheet = RuleSheet.People })
                ListRow("Tags", icon = SquirrelIcon.Tag, meta = groupMeta(form.tags.map(::tagLabel), form.tagsMatch, "Any tag"), onClick = { sheet = RuleSheet.Tags })
                ListRow("Place", icon = SquirrelIcon.MapPin, meta = placeMeta(state), onClick = { sheet = RuleSheet.Place })
                ListRow("Camera", icon = SquirrelIcon.Camera, meta = form.cameraMake?.let { make -> form.cameraModel?.let { "$make $it" } ?: make } ?: "Any camera", onClick = { sheet = RuleSheet.Camera })
            }
            SectionTitle("Kind")
            SegmentedControl(KINDS.map { it.second }, KINDS.indexOfFirst { it.first == form.kind }.coerceAtLeast(0), onSelect = { i -> vm.edit { it.copy(kind = KINDS[i].first) } })
            SectionTitle("Taken")
            val dateKeys = KeyboardOptions(autoCorrectEnabled = false)
            TextField(form.takenFrom, { v -> vm.edit { it.copy(takenFrom = v) } }, label = "First day", keyboardOptions = dateKeys)
            TextField(form.takenTo, { v -> vm.edit { it.copy(takenTo = v) } }, label = "Last day", keyboardOptions = dateKeys)
            BasicText("YYYY-MM-DD, both days included. Leave empty for no limit.", style = SquirrelTheme.type.caption)
            state.error?.let { Alert(it, tone = AlertTone.Error) }
            Button(if (state.creating) "Create album" else "Save rules", onClick = save, enabled = state.problem == null && !state.saving, full = true)
            Spacer(Modifier.height(RailClearance))
        }
    }
    when (sheet) {
        RuleSheet.People -> PeopleRuleSheet(api, state, vm, onClose = { sheet = null })
        RuleSheet.Tags -> TagsRuleSheet(state, vm, onClose = { sheet = null })
        RuleSheet.Place -> PlaceRuleSheet(state, vm, onClose = { sheet = null })
        RuleSheet.Camera -> CameraRuleSheet(state, vm, onClose = { sheet = null })
        null -> Unit
    }
}

@Composable
internal fun SectionTitle(text: String) {
    BasicText(text, style = SquirrelTheme.type.cardTitle, modifier = Modifier.semantics { heading() })
}

/** "12 photos match" (or why there is no count yet) and the newest of them. */
@Composable
private fun PreviewCard(api: SquirrelApi, state: AlbumRulesState) {
    val preview = state.preview
    val line = when {
        state.form.problem != null -> state.form.problem ?: ""
        state.previewError != null -> state.previewError
        preview == null -> "Counting…"
        else -> "${photoCount(preview.count)} ${if (preview.count == 1L) "matches" else "match"}"
    }
    Card {
        Column(verticalArrangement = Arrangement.spacedBy(Tokens.space2)) {
            BasicText(line, style = SquirrelTheme.type.label.tabular(), modifier = Modifier.semantics { liveRegion = LiveRegionMode.Polite })
            val newest = preview?.newest.orEmpty().take(PREVIEW_TILES)
            if (newest.isNotEmpty() && state.form.problem == null) {
                Row(horizontalArrangement = Arrangement.spacedBy(Tokens.space1)) {
                    newest.forEach { PreviewTile(api, it, Modifier.weight(1f)) }
                    repeat(PREVIEW_TILES - newest.size) { Spacer(Modifier.weight(1f)) }
                }
            }
        }
    }
}

@Composable
private fun PreviewTile(api: SquirrelApi, photo: AssetSummary, modifier: Modifier) {
    Thumbnail(
        url = photo.takeIf { it.thumbnail.state == ThumbnailStatus.ready }?.let { api.thumbnailUrl(it.id, it.thumbnail.version) },
        blurhash = photo.blurhash,
        contentDescription = null,
        modifier = modifier.aspectRatio(1f).clip(Shapes.tile),
    )
}

/** "Anna or Bob", "Beach and Sea", or [none] when the group is empty. */
private fun groupMeta(labels: List<String>, match: MatchMode, none: String): String = when {
    labels.isEmpty() -> none
    labels.size == 1 -> labels.single()
    else -> labels.joinToString(if (match == MatchMode.all) " and " else " or ")
}

private fun placeMeta(state: AlbumRulesState): String {
    val f = state.form
    val country = f.country ?: return "Anywhere"
    val countryLabel = (state.places + state.cities).firstOrNull { it.countryCode == country }?.let(::countryName) ?: country
    return f.city?.let { "$it, $countryLabel" } ?: f.region?.takeIf { it.isNotEmpty() }?.let { "$it, $countryLabel" } ?: countryLabel
}

/** A camera choice's label: "Canon (any model)" for a make alone. */
internal fun cameraChoiceLabel(make: String, model: String?): String = model?.let { "$make $it" } ?: "$make (any model)"
