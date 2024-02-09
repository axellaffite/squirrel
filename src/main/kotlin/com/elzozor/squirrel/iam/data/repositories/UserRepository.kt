package com.elzozor.squirrel.iam.data.repositories

import com.elzozor.squirrel.iam.data.models.UserEntity
import io.quarkus.hibernate.orm.panache.PanacheRepositoryBase
import jakarta.enterprise.context.ApplicationScoped
import java.util.*

@ApplicationScoped
class UserRepository: PanacheRepositoryBase<UserEntity, UUID>