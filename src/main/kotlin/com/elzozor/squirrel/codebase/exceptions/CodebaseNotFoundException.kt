package com.elzozor.squirrel.codebase.exceptions

import com.elzozor.squirrel.core.SquirrelException
import jakarta.ws.rs.NotFoundException

class CodebaseNotFoundException(override val message: String): NotFoundException(), SquirrelException