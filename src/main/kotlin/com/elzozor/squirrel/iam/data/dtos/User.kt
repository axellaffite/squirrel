package com.elzozor.squirrel.iam.data.dtos

import java.util.UUID

data class User(
    val id: UUID,
    val email: String,
    val username: String
)