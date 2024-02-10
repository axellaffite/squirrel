package com.elzozor.squirrel.iam.services

import com.elzozor.squirrel.iam.data.dtos.User
import com.elzozor.squirrel.iam.data.dtos.UserCreation
import com.elzozor.squirrel.iam.data.models.UserEntity
import com.elzozor.squirrel.iam.data.repositories.UserRepository
import com.elzozor.squirrel.iam.exceptions.PasswordsDoNotMatchException
import io.quarkus.elytron.security.common.BcryptUtil
import jakarta.enterprise.context.ApplicationScoped
import jakarta.transaction.Transactional

@Transactional
@ApplicationScoped
class UserManagementService(private val userRepository: UserRepository) {
    fun createUser(userCreation: UserCreation): User {
        if (userCreation.password != userCreation.passwordConfirm) {
            throw PasswordsDoNotMatchException()
        }

        val user = UserEntity().apply {
            email = userCreation.email
            username = userCreation.username
            password = hash(userCreation.password)
            roles = ""
        }

        userRepository.persist(user)

        return User(
            id = user.id,
            email = user.email,
            username = user.username
        )
    }

    private fun hash(password: String) = BcryptUtil.bcryptHash(password)
}