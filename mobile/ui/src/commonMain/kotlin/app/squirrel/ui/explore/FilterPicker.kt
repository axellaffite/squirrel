package app.squirrel.ui.explore

import androidx.compose.foundation.text.BasicText
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.produceState
import androidx.compose.ui.Modifier
import androidx.compose.ui.semantics.heading
import androidx.compose.ui.semantics.semantics
import app.squirrel.api.models.CameraGroupDto
import app.squirrel.api.models.FaceCrop
import app.squirrel.api.models.PersonDto
import app.squirrel.api.models.PhotoKindFilter
import app.squirrel.api.models.PlaceGroupDto
import app.squirrel.api.models.PlaceLevel
import app.squirrel.api.models.TagGroupDto
import app.squirrel.shared.api.SquirrelApi
import app.squirrel.shared.api.bodyOrThrow
import app.squirrel.theme.SquirrelIcon
import app.squirrel.theme.SquirrelTheme
import app.squirrel.theme.components.Alert
import app.squirrel.theme.components.AlertTone
import app.squirrel.theme.components.Icon
import app.squirrel.theme.components.ListRow
import app.squirrel.theme.components.RowGroup
import app.squirrel.theme.components.Loading
import app.squirrel.theme.components.Sheet
import app.squirrel.ui.attempt
import app.squirrel.ui.photos.UNREACHABLE

/** One choice of the picker; picking it gives [next]. */
data class FilterOption(
    val key: String,
    val label: String,
    val count: Long?,
    val icon: SquirrelIcon,
    val next: PhotoFilter,
    val chosen: Boolean,
    val face: FaceCrop? = null,
    val person: Boolean = false,
)

/** The tags, places and cameras the picker offers (people come with the screen). */
data class FilterChoices(val tags: List<TagGroupDto>, val places: List<PlaceGroupDto>, val cameras: List<CameraGroupDto>, val failed: Boolean)

/** The picker's sections, as `FilterPicker.svelte` builds them; empty sections are left out. */
fun filterSections(filter: PhotoFilter, people: List<PersonDto>, choices: FilterChoices): List<Pair<String, List<FilterOption>>> {
    val f = filter.normalized()
    val place = PhotoFilter(country = f.country, region = f.region, city = f.city)
    return listOf(
        "People" to people.filter { !it.hidden }.map { p ->
            FilterOption(p.id, personLabel(p.name), p.faceCount, SquirrelIcon.User, f.copy(person = p.id), f.person == p.id, p.cover, person = true)
        },
        "Tags" to choices.tags.map { t ->
            FilterOption(t.tag, tagLabel(t.tag), t.count, SquirrelIcon.Tag, f.copy(tag = t.tag), f.tag == t.tag)
        },
        "Places" to choices.places.map { p ->
            FilterOption(
                key = "${p.countryCode}/${p.region.orEmpty()}/${p.city.orEmpty()}",
                label = listOfNotNull(p.city, countryName(p)).joinToString(", "),
                count = p.count,
                icon = SquirrelIcon.MapPin,
                next = f.withPlace(p),
                chosen = placeFilter(p).normalized() == place,
            )
        },
        "Cameras" to choices.cameras.map { c ->
            FilterOption("${c.make}/${c.model.orEmpty()}", cameraLabel(c), c.count, SquirrelIcon.Camera, f.withCamera(c), f.cameraMake == c.make && f.cameraModel == c.model)
        },
        "Kind" to listOf(PhotoKindFilter.photo, PhotoKindFilter.screenshot).map { k ->
            FilterOption(k.value, kindLabel(k), null, SquirrelIcon.Image, f.copy(kind = k), f.kind == k)
        },
    ).filter { it.second.isNotEmpty() }
}

/** MASTER "Filter photos" sheet: people, tags, places, cameras, kind; picking one closes it. */
@Composable
fun FilterPicker(api: SquirrelApi, filter: PhotoFilter, people: List<PersonDto>, onPick: (PhotoFilter) -> Unit, onClose: () -> Unit) {
    val choices by produceState<FilterChoices?>(null) {
        val tags = attempt { api.explore.listTags().bodyOrThrow() }
        val places = attempt { api.explore.listPlaces(PlaceLevel.city).bodyOrThrow() }
        val cameras = attempt { api.explore.listCameras().bodyOrThrow() }
        value = FilterChoices(tags.orEmpty(), places.orEmpty(), cameras.orEmpty(), failed = tags == null || places == null || cameras == null)
    }
    Sheet("Filter photos", onDismiss = onClose) {
        val loaded = choices
        if (loaded?.failed == true) Alert(UNREACHABLE, tone = AlertTone.Error)
        filterSections(filter, people, loaded ?: FilterChoices(emptyList(), emptyList(), emptyList(), false)).forEach { (title, options) ->
            BasicText(title, style = SquirrelTheme.type.cardTitle, modifier = Modifier.semantics { heading() })
            RowGroup { options.forEach { option ->
                ListRow(
                    name = option.label,
                    icon = option.icon,
                    meta = option.count?.let(::photoCount),
                    onClick = {
                        onPick(option.next)
                        onClose()
                    },
                    leading = if (option.person) ({ FaceAvatar(api, option.face) }) else null,
                    trailing = if (option.chosen) ({ Icon(SquirrelIcon.Check, contentDescription = "Chosen", tint = SquirrelTheme.scheme.accent) }) else null,
                )
            } }
        }
        if (loaded == null) Loading()
    }
}
