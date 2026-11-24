package app.squirrel.ui.explore

import app.squirrel.api.models.CameraGroupDto
import app.squirrel.api.models.PhotoKindFilter
import app.squirrel.api.models.PlaceGroupDto
import app.squirrel.theme.SquirrelIcon
import kotlinx.serialization.Serializable

/** The server's limit on a filter value. */
private const val MAX_VALUE = 200

/**
 * The photo filter of the timeline and of photo search, as `web/src/lib/explore/filters.ts`:
 * a region or city needs a country, a camera model needs its make. [region] empty: places
 * without a region (as Explore lists them).
 */
@Serializable
data class PhotoFilter(
    val person: String? = null,
    val tag: String? = null,
    val country: String? = null,
    val region: String? = null,
    val city: String? = null,
    val cameraMake: String? = null,
    val cameraModel: String? = null,
    val kind: PhotoKindFilter? = null,
) {
    /** The checked filter: anything the server would refuse is left out, never sent. */
    fun normalized(): PhotoFilter {
        val country = text(country)
        val make = text(cameraMake)
        return PhotoFilter(
            person = text(person),
            tag = text(tag),
            country = country,
            region = if (country == null) null else if (region == "") "" else text(region),
            city = if (country == null) null else text(city),
            cameraMake = make,
            cameraModel = if (make == null) null else text(cameraModel),
            kind = kind?.takeIf { it == PhotoKindFilter.photo || it == PhotoKindFilter.screenshot },
        )
    }

    val isEmpty: Boolean get() = normalized() == PhotoFilter()

    /** A stable key (view model keys, list keys). */
    val key: String
        get() = with(normalized()) { listOf(person, tag, country, region, city, cameraMake, cameraModel, kind?.value).joinToString("|") { it ?: "" } }

    /** This filter with a place's parts replaced by [place]'s. */
    fun withPlace(place: PlaceGroupDto): PhotoFilter {
        val only = placeFilter(place)
        return copy(country = only.country, region = only.region, city = only.city)
    }

    fun withCamera(camera: CameraGroupDto): PhotoFilter = copy(cameraMake = camera.make, cameraModel = camera.model)

    private fun text(value: String?): String? = value?.trim()?.takeIf { it.isNotEmpty() && it.length <= MAX_VALUE }
}

/** What a chip stands for (its icon). */
enum class ChipKind(val icon: SquirrelIcon) {
    Person(SquirrelIcon.User),
    Tag(SquirrelIcon.Tag),
    Place(SquirrelIcon.MapPin),
    Camera(SquirrelIcon.Camera),
    Kind(SquirrelIcon.Image),
}

/** One part of a filter, as a chip: removing it gives [without]. */
data class Chip(val kind: ChipKind, val label: String, val without: PhotoFilter)

private val KIND_LABELS = mapOf(PhotoKindFilter.photo to "Photos only", PhotoKindFilter.screenshot to "Screenshots")

fun kindLabel(kind: PhotoKindFilter): String = KIND_LABELS[kind] ?: kind.value

/** One chip per filter part, in the web's order. [personName] names a person id (null: "Unnamed"). */
fun chips(filter: PhotoFilter, personName: (String) -> String? = { null }): List<Chip> {
    val f = filter.normalized()
    return buildList {
        f.person?.let { add(Chip(ChipKind.Person, personName(it) ?: "Unnamed", f.copy(person = null))) }
        f.tag?.let { add(Chip(ChipKind.Tag, tagLabel(it), f.copy(tag = null))) }
        f.country?.let { country ->
            val label = f.city ?: f.region?.takeIf { it.isNotEmpty() } ?: country
            add(Chip(ChipKind.Place, label, f.copy(country = null, region = null, city = null)))
        }
        f.cameraMake?.let { make ->
            add(Chip(ChipKind.Camera, f.cameraModel?.let { "$make $it" } ?: make, f.copy(cameraMake = null, cameraModel = null)))
        }
        f.kind?.let { add(Chip(ChipKind.Kind, kindLabel(it), f.copy(kind = null))) }
    }
}

/** "birthday-cake" → "Birthday cake". */
fun tagLabel(slug: String): String {
    val words = slug.replace(Regex("[-_]+"), " ").trim()
    return words.replaceFirstChar { it.uppercase() }
}

/**
 * The filter for a place as Explore lists it: a city keeps its region (empty when it has none,
 * which matches places without a region), a country stands alone.
 */
fun placeFilter(place: PlaceGroupDto): PhotoFilter {
    val city = place.city ?: return PhotoFilter(country = place.countryCode)
    return PhotoFilter(country = place.countryCode, region = place.region ?: "", city = city)
}

/** A place's name: its city, else its country (the server's name, else its code). */
fun placeName(place: PlaceGroupDto): String = place.city ?: countryName(place)

fun countryName(place: PlaceGroupDto): String = place.country ?: place.countryCode

/** "Canon EOS R6" or "Canon". */
fun cameraLabel(camera: CameraGroupDto): String = camera.model?.let { "${camera.make} $it" } ?: camera.make

/** "Anna" or "Unnamed" (MASTER). */
fun personLabel(name: String?): String = name?.trim()?.takeIf { it.isNotEmpty() } ?: "Unnamed"

/** "1 photo", "12 photos". */
fun photoCount(count: Long): String = if (count == 1L) "1 photo" else "${app.squirrel.shared.sync.formatCount(count)} photos"

/** Places grouped under their country, in the order the server ranked them. */
fun placesByCountry(places: List<PlaceGroupDto>): List<Pair<String, List<PlaceGroupDto>>> =
    places.groupBy { it.countryCode }.map { (_, group) -> countryName(group.first()) to group }
