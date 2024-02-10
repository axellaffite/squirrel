package com.elzozor.squirrel.codebase.data.dtos

import com.fasterxml.jackson.annotation.JsonIgnore
import io.quarkus.runtime.annotations.RegisterForReflection
import java.util.*

@RegisterForReflection
data class Codebase(
    val id: UUID,
    val name: String,
    val url: String,
    @get:JsonIgnore
    val authenticationToken: String? = null
)
