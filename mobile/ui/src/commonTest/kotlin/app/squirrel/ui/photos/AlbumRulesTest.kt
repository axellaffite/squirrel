package app.squirrel.ui.photos

import app.squirrel.api.models.AlbumDto
import app.squirrel.api.models.AlbumRulesDto
import app.squirrel.api.models.CameraGroupDto
import app.squirrel.api.models.MatchMode
import app.squirrel.api.models.PhotoKindFilter
import kotlinx.datetime.LocalDate
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertNull

class AlbumRulesTest {
    @Test
    fun anEmptyFormAsksForOneRuleAndBuildsNothing() {
        val form = RulesForm()
        assertEquals("Choose at least one rule.", form.problem)
        assertNull(form.toRules())
        assertEquals(false, form.hasCondition)
    }

    @Test
    fun peopleAndTagsToggleAndKeepTheirMatchMode() {
        val form = RulesForm().togglePerson("anna").togglePerson("bob").togglePerson("anna").toggleTag("beach").copy(tagsMatch = MatchMode.all)
        assertEquals(listOf("bob"), form.people)
        assertEquals(
            AlbumRulesDto(people = listOf("bob"), peopleMatch = MatchMode.any, tags = listOf("beach"), tagsMatch = MatchMode.all),
            form.toRules(),
        )
    }

    @Test
    fun noMoreThanTwentyPeopleOrTags() {
        val full = (1..MAX_RULE_PEOPLE).fold(RulesForm()) { form, i -> form.togglePerson("p$i") }
        assertEquals(MAX_RULE_PEOPLE, full.togglePerson("one more").people.size)
        val tags = (1..MAX_RULE_TAGS).fold(RulesForm()) { form, i -> form.toggleTag("t$i") }
        assertEquals(MAX_RULE_TAGS, tags.toggleTag("one more").tags.size)
        assertEquals("At most $MAX_RULE_PEOPLE people.", RulesForm(people = (0..MAX_RULE_PEOPLE).map { "p$it" }).problem)
        assertEquals("At most $MAX_RULE_TAGS tags.", RulesForm(tags = (0..MAX_RULE_TAGS).map { "t$it" }).problem)
    }

    @Test
    fun aPlaceKeepsItsEmptyRegionAndACameraModelNeedsItsMake() {
        val form = RulesForm().withPlace("FR", "", "Brest").withCamera("Canon", "R6")
        val rules = form.toRules()
        assertEquals("FR", rules?.country)
        assertEquals("", rules?.region)
        assertEquals("Brest", rules?.city)
        assertEquals("R6", rules?.cameraModel)
        val orphan = RulesForm(region = "Bretagne", city = "Brest", cameraModel = "R6", kind = PhotoKindFilter.photo).toRules()
        assertEquals(AlbumRulesDto(kind = PhotoKindFilter.photo), orphan)
        assertNull(form.withoutPlace().withoutCamera().toRules())
    }

    @Test
    fun datesAreCheckedAndInclusive() {
        assertEquals("Enter dates as YYYY-MM-DD.", RulesForm(takenFrom = "2024-13-01").problem)
        assertEquals("The first day is after the last one.", RulesForm(takenFrom = "2024-08-01", takenTo = "2024-07-01").problem)
        val rules = RulesForm(takenFrom = " 2024-07-01 ", takenTo = "2024-07-01").toRules()
        assertEquals(LocalDate(2024, 7, 1), rules?.takenFrom)
        assertEquals(LocalDate(2024, 7, 1), rules?.takenTo)
    }

    @Test
    fun savedRulesComeBackAsTheSameForm() {
        val rules = AlbumRulesDto(
            people = listOf("anna"),
            peopleMatch = MatchMode.all,
            country = "FR",
            region = "",
            kind = PhotoKindFilter.screenshot,
            takenFrom = LocalDate(2024, 1, 2),
        )
        val form = RulesForm.from(rules)
        assertEquals("2024-01-02", form.takenFrom)
        assertEquals(MatchMode.any, form.tagsMatch)
        assertEquals(rules, form.toRules())
    }

    @Test
    fun rulesReadAsOneLine() {
        val rules = AlbumRulesDto(
            people = listOf("anna", "bob"),
            peopleMatch = MatchMode.all,
            tags = listOf("birthday-cake"),
            country = "FR",
            city = "Brest",
            cameraMake = "Canon",
            kind = PhotoKindFilter.photo,
            takenFrom = LocalDate(2024, 1, 1),
        )
        val names = mapOf("anna" to "Anna")
        assertEquals(
            "Anna and Unnamed · Birthday cake · Brest · Canon · Photos only · From 2024-01-01",
            rulesSummary(rules) { names[it] },
        )
        assertEquals("Anna or Unnamed", rulesSummary(AlbumRulesDto(people = listOf("anna", "x"))) { names[it] })
        assertEquals("Until 2024-02-03", rulesSummary(AlbumRulesDto(takenTo = LocalDate(2024, 2, 3))) { null })
    }

    @Test
    fun eachCameraMakeIsOfferedAloneThenWithItsModels() {
        val cameras = listOf(
            CameraGroupDto(count = 3, make = "Canon", model = "R6"),
            CameraGroupDto(count = 2, make = "Apple", model = "iPhone 15"),
            CameraGroupDto(count = 1, make = "Canon", model = "R5"),
        )
        assertEquals(
            listOf("Canon" to null, "Canon" to "R6", "Canon" to "R5", "Apple" to null, "Apple" to "iPhone 15"),
            cameraChoices(cameras).map { it.make to it.model },
        )
        assertEquals(4L, cameraChoices(cameras).first().count)
    }

    @Test
    fun dynamicAlbumsSayItNextToTheirCount() {
        val album = AlbumDto(id = "a", itemCount = 12, name = "Beach", updatedAt = kotlin.time.Instant.parse("2026-11-20T17:05:00Z"))
        assertEquals("12 photos", albumMeta(album))
        assertEquals("12 photos · Dynamic", albumMeta(album.copy(dynamic = true)))
    }
}
