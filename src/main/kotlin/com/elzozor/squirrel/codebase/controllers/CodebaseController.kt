package com.elzozor.squirrel.codebase.controllers

import com.elzozor.squirrel.codebase.data.dtos.Codebase
import com.elzozor.squirrel.codebase.data.dtos.CodebaseCreation
import com.elzozor.squirrel.codebase.services.CodebaseService
import jakarta.ws.rs.*
import jakarta.ws.rs.core.MediaType
import java.util.*

@Path("codebases")
@Consumes(MediaType.APPLICATION_JSON)
@Produces(MediaType.APPLICATION_JSON)
class CodebaseController(private val codebaseService: CodebaseService) {
    @GET
    fun all() = codebaseService.findAll()

    @POST
    fun create(codebase: CodebaseCreation) = codebaseService.create(codebase)

    @PUT
    @Path("{id}")
    fun update(@PathParam("id") id: UUID, codebase: Codebase) = codebaseService.update(id, codebase)

    @DELETE
    @Path("{id}")
    fun delete(@PathParam("id") id: UUID) = codebaseService.delete(id)
}