package com.elzozor.squirrel.iam.controllers

import com.elzozor.squirrel.iam.data.dtos.UserCreation
import com.elzozor.squirrel.iam.services.UserManagementService
import jakarta.ws.rs.Consumes
import jakarta.ws.rs.POST
import jakarta.ws.rs.Path
import jakarta.ws.rs.Produces
import jakarta.ws.rs.core.MediaType

@Path("users")
@Consumes(MediaType.APPLICATION_JSON)
@Produces(MediaType.APPLICATION_JSON)
class UserManagementController(private val userManagementService: UserManagementService) {
    @POST
    fun createUser(userCreation: UserCreation) = userManagementService.createUser(userCreation)
}