package app.squirrel.ui.photos

import androidx.compose.foundation.text.BasicText
import androidx.compose.runtime.Composable
import app.squirrel.api.models.MatchMode
import app.squirrel.shared.api.SquirrelApi
import app.squirrel.theme.SquirrelIcon
import app.squirrel.theme.SquirrelTheme
import app.squirrel.theme.components.Button
import app.squirrel.theme.components.Icon
import app.squirrel.theme.components.ListRow
import app.squirrel.theme.components.Loading
import app.squirrel.theme.components.RowGroup
import app.squirrel.theme.components.SegmentedControl
import app.squirrel.theme.components.Sheet
import app.squirrel.ui.explore.FaceAvatar
import app.squirrel.ui.explore.countryName
import app.squirrel.ui.explore.personLabel
import app.squirrel.ui.explore.photoCount
import app.squirrel.ui.explore.tagLabel

private val MATCHES = listOf(MatchMode.any to "Any of them", MatchMode.all to "All of them")

@Composable
private fun Chosen(chosen: Boolean) {
    if (chosen) Icon(SquirrelIcon.Check, contentDescription = "Chosen", tint = SquirrelTheme.scheme.accent)
}

@Composable
private fun MatchControl(match: MatchMode, onMatch: (MatchMode) -> Unit) {
    SegmentedControl(MATCHES.map { it.second }, MATCHES.indexOfFirst { it.first == match }.coerceAtLeast(0), onSelect = { onMatch(MATCHES[it].first) })
}

/** People to pick several of, any or all of them. */
@Composable
internal fun PeopleRuleSheet(api: SquirrelApi, state: AlbumRulesState, vm: AlbumRulesViewModel, onClose: () -> Unit) {
    val form = state.form
    Sheet("People", onDismiss = onClose) {
        MatchControl(form.peopleMatch) { mode -> vm.edit { it.copy(peopleMatch = mode) } }
        BasicText("Up to $MAX_RULE_PEOPLE people.", style = SquirrelTheme.type.caption)
        when {
            !state.choicesLoaded -> Loading()
            state.people.isEmpty() -> BasicText("Nobody recognised yet.", style = SquirrelTheme.type.body)
            else -> RowGroup {
                state.people.forEach { person ->
                    ListRow(
                        name = personLabel(person.name),
                        meta = photoCount(person.faceCount),
                        onClick = { vm.edit { it.togglePerson(person.id) } },
                        leading = { FaceAvatar(api, person.cover) },
                        trailing = { Chosen(person.id in form.people) },
                    )
                }
            }
        }
        Button("Done", onClick = onClose, full = true)
    }
}

/** Tags to pick several of, any or all of them. */
@Composable
internal fun TagsRuleSheet(state: AlbumRulesState, vm: AlbumRulesViewModel, onClose: () -> Unit) {
    val form = state.form
    Sheet("Tags", onDismiss = onClose) {
        MatchControl(form.tagsMatch) { mode -> vm.edit { it.copy(tagsMatch = mode) } }
        BasicText("Up to $MAX_RULE_TAGS tags.", style = SquirrelTheme.type.caption)
        when {
            !state.choicesLoaded -> Loading()
            state.tags.isEmpty() -> BasicText("No tags yet.", style = SquirrelTheme.type.body)
            else -> RowGroup {
                state.tags.forEach { tag ->
                    ListRow(
                        name = tagLabel(tag.tag),
                        icon = SquirrelIcon.Tag,
                        meta = photoCount(tag.count),
                        onClick = { vm.edit { it.toggleTag(tag.tag) } },
                        trailing = { Chosen(tag.tag in form.tags) },
                    )
                }
            }
        }
        Button("Done", onClick = onClose, full = true)
    }
}

/** One place: anywhere, a country, or a city; picking closes the sheet. */
@Composable
internal fun PlaceRuleSheet(state: AlbumRulesState, vm: AlbumRulesViewModel, onClose: () -> Unit) {
    val form = state.form
    val pick: ((RulesForm) -> RulesForm) -> Unit = { change ->
        vm.edit(change)
        onClose()
    }
    Sheet("Place", onDismiss = onClose) {
        RowGroup {
            ListRow("Anywhere", icon = SquirrelIcon.Map, onClick = { pick { it.withoutPlace() } }, trailing = { Chosen(form.country == null) })
        }
        if (!state.choicesLoaded) Loading()
        if (state.places.isNotEmpty()) {
            SectionTitle("Countries")
            RowGroup {
                state.places.forEach { place ->
                    ListRow(
                        name = countryName(place),
                        icon = SquirrelIcon.MapPin,
                        meta = photoCount(place.count),
                        onClick = { pick { it.withPlace(place.countryCode, null, null) } },
                        trailing = { Chosen(form.country == place.countryCode && form.region == null && form.city == null) },
                    )
                }
            }
        }
        if (state.cities.isNotEmpty()) {
            SectionTitle("Cities")
            RowGroup {
                state.cities.forEach { place ->
                    val region = place.region ?: ""
                    ListRow(
                        name = listOfNotNull(place.city, countryName(place)).joinToString(", "),
                        icon = SquirrelIcon.MapPin,
                        meta = photoCount(place.count),
                        onClick = { pick { it.withPlace(place.countryCode, region, place.city) } },
                        trailing = { Chosen(form.country == place.countryCode && form.city == place.city && form.region == region) },
                    )
                }
            }
        }
    }
}

/** One camera: any, a make (any of its models), or a make and model; picking closes the sheet. */
@Composable
internal fun CameraRuleSheet(state: AlbumRulesState, vm: AlbumRulesViewModel, onClose: () -> Unit) {
    val form = state.form
    val pick: ((RulesForm) -> RulesForm) -> Unit = { change ->
        vm.edit(change)
        onClose()
    }
    Sheet("Camera", onDismiss = onClose) {
        RowGroup {
            ListRow("Any camera", icon = SquirrelIcon.Camera, onClick = { pick { it.withoutCamera() } }, trailing = { Chosen(form.cameraMake == null) })
            cameraChoices(state.cameras).forEach { camera ->
                ListRow(
                    name = cameraChoiceLabel(camera.make, camera.model),
                    icon = SquirrelIcon.Camera,
                    meta = photoCount(camera.count),
                    onClick = { pick { it.withCamera(camera.make, camera.model) } },
                    trailing = { Chosen(form.cameraMake == camera.make && form.cameraModel == camera.model) },
                )
            }
        }
        if (!state.choicesLoaded) Loading()
    }
}
