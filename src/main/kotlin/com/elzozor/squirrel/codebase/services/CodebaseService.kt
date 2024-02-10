package com.elzozor.squirrel.codebase.services

import com.elzozor.squirrel.codebase.data.dtos.Codebase
import com.elzozor.squirrel.codebase.data.dtos.CodebaseCreation
import com.elzozor.squirrel.codebase.data.models.CodebaseEntity
import com.elzozor.squirrel.codebase.data.repositories.CodebaseRepository
import com.elzozor.squirrel.codebase.exceptions.CodebaseNotFoundException
import jakarta.enterprise.context.ApplicationScoped
import jakarta.ws.rs.NotFoundException
import java.util.*

@ApplicationScoped
class CodebaseService(private val codebaseRepository: CodebaseRepository) {
    fun findAll(): List<Codebase> {
        return codebaseRepository.findAll().project(Codebase::class.java).list()
    }

    fun find(id: UUID): Codebase {
        return codebaseRepository.findDTO(id)
            .orElseThrow { NotFoundException() }
    }

    fun create(codebase: CodebaseCreation): Codebase {
        val codebaseEntity = CodebaseEntity().apply {
            url = codebase.url
            name = codebase.name
            authenticationToken = codebase.authenticationToken
        }

        codebaseRepository.persist(codebaseEntity)

        return find(codebaseEntity.id)
    }

    fun update(id: UUID, update: Codebase): Codebase {
        val codebase = codebaseRepository.findByIdOptional(id)
            .orElseThrow { CodebaseNotFoundException("Codebase does not exist") }

        codebase.apply {
            codebase.name = update.name
            codebase.url = update.url
            if (update.authenticationToken != null) {
                codebase.authenticationToken = update.authenticationToken
            }
        }

        codebaseRepository.persist(codebase)

        return Codebase(
            id = codebase.id,
            url = codebase.url,
            name = codebase.name
        )
    }

    fun delete(id: UUID) {
        codebaseRepository.deleteById(id)
    }
}