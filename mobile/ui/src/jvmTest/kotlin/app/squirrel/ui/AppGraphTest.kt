package app.squirrel.ui

import app.squirrel.api.models.DevicePlatform
import app.squirrel.shared.api.DeviceDetails
import app.squirrel.shared.auth.InMemoryTokenStore
import app.squirrel.shared.db.createJvmDriver
import io.ktor.client.engine.mock.MockEngine
import io.ktor.client.engine.mock.respond
import io.ktor.http.ContentType
import io.ktor.http.HttpHeaders
import io.ktor.http.HttpStatusCode
import io.ktor.http.headersOf
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.cancel
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.withTimeout
import kotlin.test.AfterTest
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertIs

class AppGraphTest {
    private val scope = CoroutineScope(SupervisorJob() + Dispatchers.Default)
    private val device = DeviceDetails("Pixel 8", DevicePlatform.android, "Pixel 8", "0.1.0")
    private val unauthorized = """{"error":{"code":"unauthorized","message":"x"}}"""

    @AfterTest
    fun tearDown() = scope.cancel()

    private fun graph(store: InMemoryTokenStore) =
        AppGraph(
            driver = createJvmDriver(),
            tokenStore = { store },
            httpEngine = {
                MockEngine {
                    respond(unauthorized, HttpStatusCode.Unauthorized, headersOf(HttpHeaders.ContentType, ContentType.Application.Json.toString()))
                }
            },
            device = device,
            scope = scope,
        )

    @Test
    fun withoutStoredCredentialsTheAppAsksToSignIn(): Unit = runBlocking {
        val graph = graph(InMemoryTokenStore())
        assertIs<AppState.NeedsSignIn>(withTimeout(5_000) { graph.state.first { it != AppState.Loading } })
    }

    @Test
    fun aRevokedDeviceGoesBackToSignIn(): Unit = runBlocking {
        // Credentials from an earlier run whose device was revoked on the web meanwhile.
        val driver = createJvmDriver()
        val seeded = app.squirrel.shared.settings.SettingsStore(app.squirrel.shared.db.createDatabase(driver))
        seeded.setServerUrl("https://home.example")
        seeded.setValue("account.display_name", "Ada")
        app.squirrel.shared.db.createDatabase(driver).localAssetQueries
            .insertScanned("images:1", "photo", "a.jpg", null, 1, 1, null, null, 1)
        val graph = AppGraph(
            driver = driver,
            tokenStore = { InMemoryTokenStore("sqr_old") },
            httpEngine = {
                MockEngine {
                    respond(unauthorized, HttpStatusCode.Unauthorized, headersOf(HttpHeaders.ContentType, ContentType.Application.Json.toString()))
                }
            },
            device = device,
            scope = scope,
        )

        val final = withTimeout(5_000) { graph.state.first { it is AppState.NeedsSignIn } }

        assertEquals("https://home.example", assertIs<AppState.NeedsSignIn>(final).serverUrl)
        assertEquals(null, graph.settings.value("account.display_name"))
        assertEquals(emptyMap(), graph.database.localAssetQueries.counts().executeAsList().associate { it.state to it.n })
    }

    @Test
    fun signingOutForgetsEveryImage(): Unit = runBlocking {
        val dir = java.nio.file.Files.createTempDirectory("squirrel-images")
        val cached = dir.resolve("thumbnail.bin").toFile().apply { writeText("private") }
        var cleared = 0
        val images = object : app.squirrel.ui.images.ImagesPlatform {
            override val context = coil3.PlatformContext.INSTANCE
            override val cacheDir = okio.Path.Companion.run { dir.toString().toPath() }
            override fun clearFiles() {
                cleared++
                dir.toFile().listFiles()?.forEach { it.deleteRecursively() }
            }
        }
        val driver = createJvmDriver()
        val seeded = app.squirrel.shared.settings.SettingsStore(app.squirrel.shared.db.createDatabase(driver))
        seeded.setServerUrl("https://home.example")
        seeded.setValue("account.display_name", "Ada")
        val graph = AppGraph(
            driver = driver,
            tokenStore = { InMemoryTokenStore("sqr_old") },
            httpEngine = {
                MockEngine {
                    respond(unauthorized, HttpStatusCode.Unauthorized, headersOf(HttpHeaders.ContentType, ContentType.Application.Json.toString()))
                }
            },
            device = device,
            scope = scope,
            images = images,
        )

        withTimeout(5_000) { graph.state.first { it is AppState.NeedsSignIn } } // the device was revoked

        assertEquals(null, graph.imageLoader.value)
        assertEquals(1, cleared)
        assertEquals(false, cached.exists())
    }

    @Test
    fun anotherAccountGetsFreshViewModels(): Unit = runBlocking {
        val graph = graph(InMemoryTokenStore())
        withTimeout(5_000) { graph.state.first { it is AppState.NeedsSignIn } }
        val api = graph.newApi("https://home.example")
        fun signedIn(name: String) = app.squirrel.shared.api.SignedIn(
            app.squirrel.api.models.AccountDto(displayName = name, id = name, role = app.squirrel.api.models.Role.user, username = name),
            app.squirrel.api.models.DeviceDto(
                appVersion = "0.1.0",
                createdAt = kotlin.time.Instant.fromEpochMilliseconds(0),
                current = true,
                id = "d",
                lastSeenAt = kotlin.time.Instant.fromEpochMilliseconds(0),
                name = "Pixel",
                platform = app.squirrel.api.models.DevicePlatform.android,
            ),
        )
        graph.onSignedIn("https://home.example", api, signedIn("ada"))
        val first = (graph.state.value as AppState.SignedIn).scope
        var cleared = false
        val photos = object : androidx.lifecycle.ViewModel() {
            override fun onCleared() { cleared = true }
        }
        first.viewModelStore.put("timeline", photos)

        graph.signOut()
        withTimeout(5_000) { graph.state.first { it is AppState.NeedsSignIn } }
        graph.onSignedIn("https://home.example", graph.newApi("https://home.example"), signedIn("bob"))

        val second = (graph.state.value as AppState.SignedIn).scope
        assertEquals(true, cleared) // Ada's view models are gone ...
        kotlin.test.assertNotSame(first, second)
        assertEquals(emptySet(), second.viewModelStore.keys()) // ... and Bob starts with none
    }

}
