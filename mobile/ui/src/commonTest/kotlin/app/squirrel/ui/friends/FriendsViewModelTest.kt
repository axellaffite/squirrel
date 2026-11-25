package app.squirrel.ui.friends

import app.squirrel.ui.photos.UNREACHABLE
import app.squirrel.ui.testing.await
import app.squirrel.ui.testing.runReal
import app.squirrel.ui.testing.testApi
import io.ktor.http.HttpStatusCode
import io.ktor.client.engine.mock.respondError
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.test.resetMain
import kotlinx.coroutines.test.setMain
import kotlin.test.AfterTest
import kotlin.test.BeforeTest
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertNull

@OptIn(ExperimentalCoroutinesApi::class)
class FriendsViewModelTest {
    @BeforeTest
    fun setUp() = Dispatchers.setMain(Dispatchers.Unconfined)

    @AfterTest
    fun tearDown() = Dispatchers.resetMain()

    private val server = FriendsServer()

    private suspend fun vm() = FriendsViewModel(testApi(server.handler()))

    @Test
    fun friendsAndSuggestionsLoad() = runReal {
        val state = vm().state.await { it.friends != null }
        assertEquals(listOf("anna"), state.friends?.map { it.username })
        assertEquals(listOf("bob", "cleo"), state.suggestions.map { it.username })
        assertNull(state.error)
    }

    @Test
    fun addingByUsernameEmptiesTheFieldAndTakesTheSuggestionOut() = runReal {
        val vm = vm()
        vm.state.await { it.friends != null }
        vm.setUsername("  bob ")
        assertNull(vm.addTyped())
        val state = vm.state.value
        assertEquals(listOf("bob"), server.added)
        assertEquals(listOf("anna", "bob"), state.friends?.map { it.username })
        assertEquals(listOf("cleo"), state.suggestions.map { it.username })
        assertEquals("", state.username)
        assertEquals(false, state.busy)
    }

    @Test
    fun theServersRefusalShowsUnderTheFieldAndKeepsIt() = runReal {
        val vm = vm()
        vm.state.await { it.friends != null }
        assertEquals("Enter a username.", vm.addTyped())
        assertEquals("Enter a username.", vm.state.value.addError)
        assertEquals(emptyList(), server.added)
        vm.setUsername("nobody")
        assertNull(vm.state.value.addError, "typing clears the message")
        assertEquals("No account is called nobody.", vm.addTyped())
        assertEquals("No account is called nobody.", vm.state.value.addError)
        assertEquals("nobody", vm.state.value.username)
        assertEquals(listOf("anna"), vm.state.value.friends?.map { it.username })
    }

    @Test
    fun aSuggestionIsAddedInOneTapWithoutTouchingTheField() = runReal {
        val vm = vm()
        vm.state.await { it.friends != null }
        vm.setUsername("dan")
        assertNull(vm.addSuggested("cleo"))
        assertEquals(listOf("anna", "cleo"), vm.state.value.friends?.map { it.username })
        assertEquals(listOf("bob"), vm.state.value.suggestions.map { it.username })
        assertEquals("dan", vm.state.value.username)
    }

    @Test
    fun removingTakesAFriendOffTheList() = runReal {
        val vm = vm()
        vm.state.await { it.friends != null }
        assertNull(vm.remove("anna"))
        assertEquals(listOf("anna"), server.removed)
        assertEquals(emptyList(), vm.state.value.friends)
    }

    @Test
    fun aRemovedFriendYouShareWithIsBackInSuggestionsAtOnce() = runReal {
        val vm = vm()
        vm.state.await { it.friends != null }
        assertNull(vm.remove("anna"))
        val state = vm.state.value
        assertEquals(listOf("anna", "bob", "cleo"), state.suggestions.map { it.username })
        assertEquals(false, state.busy)
        assertEquals(false, state.loading)
    }

    @Test
    fun removingSomeoneNotAFriendAnyMoreDropsTheRowQuietly() = runReal {
        val server = FriendsServer(friends = listOf("anna", "bob"))
        val vm = FriendsViewModel(testApi(server.handler()))
        vm.state.await { it.friends != null }
        server.forget("bob")
        assertNull(vm.remove("bob"))
        assertEquals(listOf("anna"), vm.state.value.friends?.map { it.username })
        assertNull(vm.state.value.error)
    }

    @Test
    fun aFailedLoadSaysWhy() = runReal {
        val vm = FriendsViewModel(testApi { respondError(HttpStatusCode.BadGateway) })
        assertEquals(UNREACHABLE, vm.state.await { !it.loading && it.error != null }.error)
        assertNull(vm.state.value.friends)
    }

    @Test
    fun theLaunchingCallsDoTheSame() = runReal {
        val vm = vm()
        vm.state.await { it.friends != null }
        vm.setUsername("bob")
        vm.submit()
        vm.state.await { it.friends?.size == 2 }
        vm.pick("cleo")
        vm.state.await { it.friends?.size == 3 }
        vm.drop("anna")
        assertEquals(listOf("bob", "cleo"), vm.state.await { it.friends?.size == 2 }.friends?.map { it.username })
    }
}
