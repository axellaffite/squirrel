package com.elzozor.squirrel.iam.exceptions

import com.elzozor.squirrel.core.SquirrelException
import jakarta.ws.rs.BadRequestException

open class InvalidUserException(override val message: String): BadRequestException(), SquirrelException