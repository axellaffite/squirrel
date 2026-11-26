package app.squirrel.ui.photos

import app.squirrel.api.models.AlbumRulesDto
import app.squirrel.api.models.CameraGroupDto
import app.squirrel.api.models.MatchMode
import app.squirrel.api.models.PhotoKindFilter
import app.squirrel.ui.explore.kindLabel
import app.squirrel.ui.explore.personLabel
import app.squirrel.ui.explore.tagLabel
import kotlinx.datetime.LocalDate

/** The server's limits on a dynamic album's people and tags. */
const val MAX_RULE_PEOPLE = 20
const val MAX_RULE_TAGS = 20

private const val NO_RULE = "Choose at least one rule."
private const val BAD_DATE = "Enter dates as YYYY-MM-DD."
private const val DATES_REVERSED = "The first day is after the last one."

/**
 * A dynamic album's rules while they are edited: groups combined with AND, each empty one
 * constraining nothing. [region] empty: places without a region. Dates are typed text
 * (`YYYY-MM-DD`, both days included); [toRules] gives what the API takes, or null while a
 * [problem] stands.
 */
data class RulesForm(
    val people: List<String> = emptyList(),
    val peopleMatch: MatchMode = MatchMode.any,
    val tags: List<String> = emptyList(),
    val tagsMatch: MatchMode = MatchMode.any,
    val country: String? = null,
    val region: String? = null,
    val city: String? = null,
    val cameraMake: String? = null,
    val cameraModel: String? = null,
    val kind: PhotoKindFilter? = null,
    val takenFrom: String = "",
    val takenTo: String = "",
) {
    /** In or out of the people; never past [MAX_RULE_PEOPLE]. */
    fun togglePerson(id: String): RulesForm = copy(people = toggled(people, id, MAX_RULE_PEOPLE))

    /** In or out of the tags; never past [MAX_RULE_TAGS]. */
    fun toggleTag(tag: String): RulesForm = copy(tags = toggled(tags, tag, MAX_RULE_TAGS))

    fun withPlace(country: String, region: String?, city: String?): RulesForm = copy(country = country, region = region, city = city)

    fun withoutPlace(): RulesForm = copy(country = null, region = null, city = null)

    fun withCamera(make: String, model: String?): RulesForm = copy(cameraMake = make, cameraModel = model)

    fun withoutCamera(): RulesForm = copy(cameraMake = null, cameraModel = null)

    /** The rules as the API takes them, empty groups and orphans (a city without a country) left out. */
    private fun built(): AlbumRulesDto {
        val country = text(country)
        val make = text(cameraMake)
        return AlbumRulesDto(
            people = people.ifEmpty { null },
            peopleMatch = peopleMatch.takeIf { people.isNotEmpty() },
            tags = tags.ifEmpty { null },
            tagsMatch = tagsMatch.takeIf { tags.isNotEmpty() },
            country = country,
            region = if (country == null) null else if (region == "") "" else text(region),
            city = if (country == null) null else text(city),
            cameraMake = make,
            cameraModel = if (make == null) null else text(cameraModel),
            kind = kind?.takeIf { it == PhotoKindFilter.photo || it == PhotoKindFilter.screenshot },
            takenFrom = day(takenFrom),
            takenTo = day(takenTo),
        )
    }

    val hasCondition: Boolean get() = built() != AlbumRulesDto() || takenFrom.isNotBlank() || takenTo.isNotBlank()

    /** What stops these rules from being saved, or null. */
    val problem: String?
        get() {
            val first = day(takenFrom)
            val last = day(takenTo)
            return when {
                !hasCondition -> NO_RULE
                people.size > MAX_RULE_PEOPLE -> "At most $MAX_RULE_PEOPLE people."
                tags.size > MAX_RULE_TAGS -> "At most $MAX_RULE_TAGS tags."
                (takenFrom.isNotBlank() && first == null) || (takenTo.isNotBlank() && last == null) -> BAD_DATE
                first != null && last != null && first > last -> DATES_REVERSED
                else -> null
            }
        }

    fun toRules(): AlbumRulesDto? = if (problem == null) built() else null

    companion object {
        /** Saved rules, ready to edit. */
        fun from(rules: AlbumRulesDto): RulesForm = RulesForm(
            people = rules.people.orEmpty(),
            peopleMatch = rules.peopleMatch.known(),
            tags = rules.tags.orEmpty(),
            tagsMatch = rules.tagsMatch.known(),
            country = rules.country,
            region = rules.region,
            city = rules.city,
            cameraMake = rules.cameraMake,
            cameraModel = rules.cameraModel,
            kind = rules.kind,
            takenFrom = rules.takenFrom?.toString().orEmpty(),
            takenTo = rules.takenTo?.toString().orEmpty(),
        )

        private fun MatchMode?.known() = if (this == MatchMode.all) MatchMode.all else MatchMode.any
    }
}

private fun toggled(list: List<String>, value: String, max: Int): List<String> = when {
    value in list -> list - value
    list.size >= max -> list
    else -> list + value
}

private fun text(value: String?): String? = value?.trim()?.takeIf { it.isNotEmpty() }

/** A `YYYY-MM-DD` day, or null (blank or not a day). */
private fun day(value: String): LocalDate? {
    val text = value.trim()
    if (text.isEmpty()) return null
    return try {
        LocalDate.parse(text)
    } catch (_: IllegalArgumentException) {
        null
    }
}

/** "Anna and Bob · Beach · Brest · Canon · Photos only · From 2024-01-01" ([personName] null: "Unnamed"). */
fun rulesSummary(rules: AlbumRulesDto, personName: (String) -> String?): String = buildList {
    rules.people?.takeIf { it.isNotEmpty() }?.let { add(joined(it.map { id -> personLabel(personName(id)) }, rules.peopleMatch)) }
    rules.tags?.takeIf { it.isNotEmpty() }?.let { add(joined(it.map(::tagLabel), rules.tagsMatch)) }
    rules.country?.let { country -> add(rules.city ?: rules.region?.takeIf { it.isNotEmpty() } ?: country) }
    rules.cameraMake?.let { make -> add(rules.cameraModel?.let { "$make $it" } ?: make) }
    rules.kind?.let { add(kindLabel(it)) }
    val from = rules.takenFrom
    val to = rules.takenTo
    when {
        from != null && to != null -> add("$from to $to")
        from != null -> add("From $from")
        to != null -> add("Until $to")
    }
}.joinToString(" · ")

/** "Anna, Bob and Carol" (all of them) or "Anna, Bob or Carol" (any). */
private fun joined(names: List<String>, match: MatchMode?): String {
    if (names.size == 1) return names.single()
    val word = if (match == MatchMode.all) "and" else "or"
    return "${names.dropLast(1).joinToString(", ")} $word ${names.last()}"
}

/**
 * The cameras a rule can name: each make on its own (any of its models, counts added up), then
 * its models, in the server's order.
 */
fun cameraChoices(cameras: List<CameraGroupDto>): List<CameraGroupDto> =
    cameras.groupBy { it.make }.flatMap { (make, group) ->
        val models = group.filter { it.model != null }
        listOf(CameraGroupDto(count = group.sumOf { it.count }, make = make)) + models
    }
