package app.squirrel.ui.sharing

import app.squirrel.ui.testing.await
import app.squirrel.ui.testing.json
import app.squirrel.ui.testing.node
import app.squirrel.ui.testing.runReal
import app.squirrel.ui.testing.summary
import app.squirrel.ui.testing.testApi
import io.ktor.client.engine.mock.respond
import io.ktor.client.engine.mock.respondError
import io.ktor.http.HttpMethod
import io.ktor.http.HttpStatusCode
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.test.resetMain
import kotlinx.coroutines.test.setMain
import kotlin.test.AfterTest
import kotlin.test.BeforeTest
import kotlin.test.Test
import kotlin.test.assertEquals

@OptIn(ExperimentalCoroutinesApi::class)
class SharedViewModelTest {
    @BeforeTest
    fun setUp() = Dispatchers.setMain(Dispatchers.Unconfined)

    @AfterTest
    fun tearDown() = Dispatchers.resetMain()

    private val owner = """{"username":"anna","display_name":"Anna"}"""
    private val album = """{"id":"al","name":"Holidays","item_count":3,"updated_at":"2026-11-20T17:05:00Z","access":"write","cover":${summary("p")},"shared_by":$owner}"""

    @Test
    fun itemsAreSplitAndLeavingAskedFirst() = runReal {
        val vm = SharedViewModel(testApi { request ->
            when {
                request.url.encodedPath == "/api/shared-with-me" -> json(
                    """[{"share_id":"s1","owner":$owner,"permission":"read","shared_at":"2026-11-20T17:05:00Z","node":${node("docs", "folder", parent = null)}},""" +
                        """{"share_id":"s2","owner":$owner,"permission":"write","shared_at":"2026-11-20T17:05:00Z","album":$album}]""",
                )
                request.url.encodedPath == "/api/shares/s2" && request.method == HttpMethod.Delete -> respond("", HttpStatusCode.NoContent)
                else -> respondError(HttpStatusCode.NotFound)
            }
        })
        val state = vm.state.await { it.items != null }
        assertEquals(listOf("docs"), state.nodes.map { it.node?.name })
        assertEquals(listOf("Holidays"), state.albums.map { it.album?.name })
        assertEquals("Holidays", subjectName(state.albums.single()))
        vm.askLeave(state.albums.single())
        assertEquals("s2", vm.state.value.leaving?.shareId)
        vm.confirmLeave()
        val left = vm.state.await { it.leaving == null && it.items?.size == 1 }
        assertEquals(listOf("s1"), left.items.orEmpty().map { it.shareId })
    }

    private val bob = """{"username":"bob","display_name":"Bob"}"""
    private val mine = """{"id":"mine","name":"Party","item_count":0,"updated_at":"2026-11-20T17:05:00Z","access":"owner"}"""

    @Test
    fun whatTheCallerSharedIsListedAndManagingReloadsIt() = runReal {
        var byMeLoads = 0
        val vm = SharedViewModel(testApi { request ->
            when (request.url.encodedPath) {
                "/api/shared-with-me" -> json("[]")
                "/api/shared-by-me" -> {
                    byMeLoads++
                    json(
                        """[{"people":[{"id":"s1","person":$bob,"permission":"read","created_at":"2026-11-20T17:05:00Z"}],"shared_at":"2026-11-20T17:05:00Z","node":${node("trip", "folder")}},""" +
                            """{"people":[{"id":"s2","person":$bob,"permission":"write","created_at":"2026-11-20T17:05:00Z"}],"shared_at":"2026-11-20T17:05:00Z","album":$mine}]""",
                    )
                }
                else -> respondError(HttpStatusCode.NotFound)
            }
        })
        val state = vm.state.await { it.byMe != null && !it.loading }
        assertEquals(listOf("trip"), state.byMeNodes.map { it.node?.name })
        assertEquals(listOf("Party"), state.byMeAlbums.map { it.album?.name })
        vm.manage(state.byMeAlbums.single())
        assertEquals("Party", vm.state.value.managing?.album?.name)
        vm.doneManaging()
        val reloaded = vm.state.await { it.managing == null && !it.loading && byMeLoads == 2 }
        assertEquals(2, reloaded.byMe?.size)
    }

    @Test
    fun aFailureKeepsTheListAndSaysWhy() = runReal {
        val vm = SharedViewModel(testApi { respondError(HttpStatusCode.BadGateway) })
        assertEquals(app.squirrel.ui.photos.UNREACHABLE, vm.state.await { !it.loading && it.error != null }.error)
    }
}
