package com.elzozor.squirrel.iam.data.models

import io.quarkus.hibernate.orm.panache.PanacheEntity
import io.quarkus.security.jpa.Password
import io.quarkus.security.jpa.Roles
import io.quarkus.security.jpa.UserDefinition
import io.quarkus.security.jpa.Username
import jakarta.persistence.Entity
import jakarta.persistence.Id
import jakarta.persistence.Table
import java.util.UUID

@Entity
@Table(name = "users")
@UserDefinition
class UserEntity {
    @Id
    var id: UUID = UUID.randomUUID()

    @Username
    lateinit var username: String

    @Password
    lateinit var password: String

    lateinit var email: String

    @Roles
    lateinit var roles: String
}