package com.elzozor.squirrel.codebase.data.repositories

import com.elzozor.squirrel.codebase.data.dtos.Codebase
import com.elzozor.squirrel.codebase.data.models.CodebaseEntity
import io.quarkus.hibernate.orm.panache.PanacheRepositoryBase
import jakarta.enterprise.context.ApplicationScoped
import java.util.*

@ApplicationScoped
class CodebaseRepository: PanacheRepositoryBase<CodebaseEntity, UUID> {
    fun findDTO(id: UUID): Optional<Codebase> {
        return find("id", id).project(Codebase::class.java).singleResultOptional()
    }
}