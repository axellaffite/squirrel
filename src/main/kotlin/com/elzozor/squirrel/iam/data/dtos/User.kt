package com.elzozor.squirrel.iam.data.dtos

import java.util.*

data class User(
    val id: UUID,
    val email: String,
    val username: String
)