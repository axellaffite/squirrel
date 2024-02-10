package com.elzozor.squirrel.codebase.services

import com.elzozor.squirrel.codebase.data.dtos.Codebase
import com.elzozor.squirrel.codebase.data.dtos.CodebaseCreation
import com.elzozor.squirrel.codebase.data.repositories.CodebaseRepository
import com.elzozor.squirrel.codebase.exceptions.CodebaseNotFoundException
import integration.PostgresContainer
import io.quarkus.test.TestTransaction
import io.quarkus.test.common.QuarkusTestResource
import io.quarkus.test.junit.QuarkusTest
import io.quarkus.test.junit.mockito.InjectSpy
import io.restassured.RestAssured.given
import jakarta.ws.rs.core.MediaType
import org.junit.jupiter.api.Assertions.*
import org.junit.jupiter.api.Test
import org.junit.jupiter.api.assertThrows
import org.mockito.Mockito.verify
import org.mockito.kotlin.clearInvocations
import org.mockito.kotlin.eq
import org.mockito.kotlin.verifyNoMoreInteractions
import java.util.*

@QuarkusTest
@QuarkusTestResource(PostgresContainer::class)
@TestTransaction
class CodebaseServiceTest {

    @InjectSpy
    private lateinit var codebaseService: CodebaseService
    @InjectSpy
    private lateinit var codebaseRepository: CodebaseRepository

    @Test
    fun createCodebase() {
        // Given a codebase
        val codebaseCreation = CodebaseCreation("Gitlab", "https://gitlab.com", "no-auth")

        // When it is inserted into the database
        val codebase = codebaseService.create(codebaseCreation)

        // Then, an entry is stored in the database
        assertTrue(codebaseRepository.findByIdOptional(codebase.id).isPresent)
    }

    @Test
    fun updateCodebase() {
        // Given an existing codebase
        val existing = codebaseService.create(
            CodebaseCreation("Gitlab", "https://gitlab.com", "no-auth")
        )

        // When it is updated in the database
        codebaseService.update(existing.id, existing.copy(name = "updated"))

        // Then we now have the updated instance in the database
        val expectedInstanceCount = 1L
        assertEquals(expectedInstanceCount, codebaseRepository.find("name", "updated").count())
    }

    @Test
    fun updateNonExistingCodebase() {
        // Given a database where the updated codebase does not exist
        assertEquals(0, codebaseRepository.count())
        clearInvocations(codebaseRepository)

        // When it is updated
        val id = UUID.randomUUID()
        val update = Codebase(id = id, name = "", url = "", authenticationToken = null)

        // Then an exception is thrown
        assertThrows<CodebaseNotFoundException> {
            codebaseService.update(id, update)
        }

        verify(codebaseRepository).findByIdOptional(eq(id))
        verifyNoMoreInteractions(codebaseRepository)
    }

    @Test
    fun deleteCodebase() {
        // Given an existing codebase
        val existing = codebaseService.create(
            CodebaseCreation("Gitlab", "https://gitlab.com", "no-auth")
        )

        // When it is removed from the database
        codebaseService.delete(existing.id)

        // Then it is not present anymore
        val expectedInstanceCount = 0L
        assertEquals(expectedInstanceCount, codebaseRepository.count())
    }

    @Test
    fun deleteNonExistingCodebase() {
        // Given a database that does not contain the codebase to remove
        assertEquals(0, codebaseRepository.count())

        // When it is removed from the database
        // Then, no exception is thrown
        assertDoesNotThrow {
            codebaseService.delete(UUID.randomUUID())
        }
    }

    @Test
    fun updateNonExistingCodebaseHttp() {
        given()
            .pathParam("id", UUID.randomUUID())
            .contentType(MediaType.APPLICATION_JSON)
            .body(Codebase(id = UUID.randomUUID(), name = "", url = "", authenticationToken = null))
            .put("codebases/{id}")
            .then()
            .statusCode(404)
    }
}