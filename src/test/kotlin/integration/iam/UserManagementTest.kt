package integration.iam

import com.elzozor.squirrel.iam.data.dtos.UserCreation
import com.elzozor.squirrel.iam.data.repositories.UserRepository
import com.elzozor.squirrel.iam.exceptions.PasswordsDoNotMatchException
import com.elzozor.squirrel.iam.services.UserManagementService
import integration.PostgresContainer
import io.quarkus.test.TestTransaction
import io.quarkus.test.common.QuarkusTestResource
import io.quarkus.test.junit.QuarkusTest
import jakarta.inject.Inject
import org.junit.jupiter.api.Assertions.assertEquals
import org.junit.jupiter.api.Assertions.assertTrue
import org.junit.jupiter.api.Test
import org.junit.jupiter.api.assertThrows

@QuarkusTest
@QuarkusTestResource(PostgresContainer::class)
@TestTransaction
class UserManagementTest {
    @Inject
    private lateinit var userManagementService: UserManagementService

    @Inject
    private lateinit var userRepository: UserRepository

    @Test
    fun userCreation() {
        // Given valid user information
        val userInfo = UserCreation("email@email.fr", "username", "password", "password")

        // When this user is persisted
        val user = userManagementService.createUser(userInfo)

        // Then it must be stored in the database
        assertTrue(userRepository.findByIdOptional(user.id).isPresent)
    }

    @Test
    fun userCreationPasswordsDoNotMatch() {
        // Given user information where passwords do not match
        val userInfo = UserCreation("email@email.fr", "username", "password", "not matching")

        // When this user is persisted
        // Then an appropriate exception must be thrown
        assertThrows<PasswordsDoNotMatchException> {
            userManagementService.createUser(userInfo)
        }

        // And it must not be present in the database
        val expectedUsersCount = 0L
        assertEquals(expectedUsersCount, userRepository.count())
    }
}