package app.squirrel.shared.api

import app.squirrel.api.models.DeviceInfo
import app.squirrel.api.models.DeviceLoginRequest
import app.squirrel.api.models.DevicePlatform
import app.squirrel.api.models.LoginRequest
import app.squirrel.api.models.RefreshRequest
import app.squirrel.api.models.TokenPair
import app.squirrel.shared.auth.Tokens
import kotlin.test.Test
import kotlin.test.assertFalse
import kotlin.time.Instant

class RedactionTest {
    private val secrets = listOf("hunter2", "sqa_secret", "sqr_secret", "squ_secret")

    private fun assertRedacted(value: Any) {
        val printed = value.toString()
        secrets.forEach { assertFalse(it in printed, "$printed leaks $it") }
    }

    @Test
    fun secretsNeverPrint() {
        val at = Instant.fromEpochMilliseconds(0)
        assertRedacted(Tokens("sqa_secret", "sqr_secret"))
        assertRedacted(ChunkAuth.UploadToken("squ_secret"))
        assertRedacted(TokenPair(accessExpiresAt = at, accessToken = "sqa_secret", refreshExpiresAt = at, refreshToken = "sqr_secret"))
        assertRedacted(RefreshRequest("sqr_secret"))
        assertRedacted(LoginRequest(password = "hunter2", username = "ada"))
        assertRedacted(
            DeviceLoginRequest(
                device = DeviceInfo(appVersion = "0.1.0", name = "Pixel", platform = DevicePlatform.android),
                password = "hunter2",
                username = "ada",
            ),
        )
    }
}
