package app.squirrel.ui.explore

import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.text.BasicText
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.heading
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.style.TextOverflow
import app.squirrel.api.models.CameraGroupDto
import app.squirrel.api.models.PersonDto
import app.squirrel.api.models.PlaceGroupDto
import app.squirrel.api.models.TagGroupDto
import app.squirrel.shared.api.SquirrelApi
import app.squirrel.theme.Shapes
import app.squirrel.theme.SquirrelIcon
import app.squirrel.theme.SquirrelTheme
import app.squirrel.theme.Tokens
import app.squirrel.theme.components.Button
import app.squirrel.theme.components.ButtonSize
import app.squirrel.theme.components.ButtonVariant
import app.squirrel.theme.components.ListRow
import app.squirrel.theme.components.RowGroup
import app.squirrel.theme.tabular
import app.squirrel.ui.photos.CoverCard
import app.squirrel.ui.photos.TwoColumns

/** People per row on phones (MASTER "People grid"). */
private const val PEOPLE_COLUMNS = 3

/** Opens the timeline filtered by one thing, titled [title]. */
typealias OpenFilter = (filter: PhotoFilter, title: String) -> Unit

/** A section heading with an optional quiet "See all". */
@Composable
fun SectionHead(title: String, onSeeAll: (() -> Unit)?) {
    Row(Modifier.fillMaxWidth().padding(top = Tokens.space3), verticalAlignment = Alignment.CenterVertically) {
        BasicText(title, style = SquirrelTheme.type.sectionTitle, modifier = Modifier.weight(1f).semantics { heading() })
        if (onSeeAll != null) Button("See all", onClick = onSeeAll, variant = ButtonVariant.Quiet, size = ButtonSize.Sm)
    }
}

/** MASTER "People grid": three columns, a `--face-lg` avatar over the name and the count. */
@Composable
fun PeopleGrid(api: SquirrelApi, people: List<PersonDto>, onOpen: OpenFilter) {
    Column(verticalArrangement = Arrangement.spacedBy(Tokens.space3)) {
        people.chunked(PEOPLE_COLUMNS).forEach { row ->
            Row(horizontalArrangement = Arrangement.spacedBy(Tokens.space2)) {
                row.forEach { person -> PersonCell(api, person, Modifier.weight(1f)) { onOpen(PhotoFilter(person = person.id), personLabel(person.name)) } }
                repeat(PEOPLE_COLUMNS - row.size) { Box(Modifier.weight(1f)) }
            }
        }
    }
}

@Composable
private fun PersonCell(api: SquirrelApi, person: PersonDto, modifier: Modifier, onClick: () -> Unit) {
    val type = SquirrelTheme.type
    val named = !person.name.isNullOrBlank()
    Column(
        modifier = modifier.clip(Shapes.card).clickable(role = Role.Button, onClickLabel = "Show photos", onClick = onClick).padding(Tokens.space1),
        horizontalAlignment = Alignment.CenterHorizontally,
        verticalArrangement = Arrangement.spacedBy(Tokens.space1),
    ) {
        FaceAvatar(api, person.cover, size = Tokens.faceLg)
        BasicText(
            personLabel(person.name),
            style = (if (named) type.label else type.label.copy(color = SquirrelTheme.scheme.inkMuted)).copy(textAlign = TextAlign.Center),
            maxLines = 1,
            overflow = TextOverflow.Ellipsis,
        )
        BasicText(photoCount(person.faceCount), style = type.caption.tabular().copy(textAlign = TextAlign.Center), maxLines = 1)
    }
}

@Composable
fun PlaceCards(api: SquirrelApi, places: List<PlaceGroupDto>, onOpen: OpenFilter) {
    TwoColumns(places) { place, modifier ->
        val name = placeName(place)
        CoverCard(
            api, place.cover, name, photoCount(place.count),
            onClick = { onOpen(placeFilter(place), name) },
            modifier = modifier,
            icon = SquirrelIcon.MapPin,
        )
    }
}

@Composable
fun TagCards(api: SquirrelApi, tags: List<TagGroupDto>, onOpen: OpenFilter) {
    TwoColumns(tags) { tag, modifier ->
        val name = tagLabel(tag.tag)
        CoverCard(api, tag.cover, name, photoCount(tag.count), onClick = { onOpen(PhotoFilter(tag = tag.tag), name) }, modifier = modifier, icon = SquirrelIcon.Tag)
    }
}

@Composable
fun CameraRows(cameras: List<CameraGroupDto>, onOpen: OpenFilter) {
    RowGroup {
        cameras.forEach { camera ->
            val name = cameraLabel(camera)
            ListRow(name, icon = SquirrelIcon.Camera, meta = photoCount(camera.count), onClick = { onOpen(PhotoFilter(cameraMake = camera.make, cameraModel = camera.model), name) })
        }
    }
}
