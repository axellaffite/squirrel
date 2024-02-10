package com.elzozor.squirrel.codebase.data.models

import jakarta.persistence.Entity
import jakarta.persistence.Id
import jakarta.persistence.Table
import java.util.*

@Entity
@Table(name = "code-managers")
class CodebaseEntity {
    @Id
    var id: UUID = UUID.randomUUID()

    lateinit var name: String
    lateinit var url: String
    lateinit var authenticationToken: String
}