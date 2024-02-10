package integration.iam

import com.elzozor.squirrel.iam.data.dtos.UserCreation
import com.elzozor.squirrel.iam.services.UserManagementService
import io.quarkus.test.junit.QuarkusTest
import io.restassured.RestAssured.given
import jakarta.inject.Inject
import org.junit.jupiter.api.Test

@QuarkusTest
class AuthenticationTest {
    @Inject
    private lateinit var userManagementService: UserManagementService

    @Test
    fun successfulAuthentication() {
        // Given a user stored in database
        userManagementService.createUser(UserCreation(
            email = "email@email.fr",
            username = "elzozor",
            password = "integration",
            passwordConfirm = "integration",
        ))

        given()
            .formParams(mapOf(
                "j_username" to "elzozor",
                "j_password" to "integration"
            ))
            .post("/auth/form")
            .then()
            .statusCode(200)
            .cookie("squirrel-credentials")
    }
}