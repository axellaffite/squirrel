package com.elzozor.squirrel.iam.data.dtos

class UserCreation(
    val email: String,
    val username: String,
    val password: String,
    val passwordConfirm: String
)