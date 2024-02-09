package com.elzozor.squirrel.iam.controllers

import com.elzozor.squirrel.iam.services.UserManagementService
import com.elzozor.squirrel.iam.data.dtos.UserCreation
import jakarta.ws.rs.POST
import jakarta.ws.rs.Path

@Path("users")
class UserManagementController(private val userManagementService: UserManagementService) {
    @POST
    fun createUser(userCreation: UserCreation) = userManagementService.createUser(userCreation)
}