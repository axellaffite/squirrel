package app.squirrel.shared.testing

import app.cash.sqldelight.db.SqlDriver
import app.cash.sqldelight.driver.native.inMemoryDriver
import app.squirrel.shared.db.SquirrelDatabase

actual fun testDriver(): SqlDriver = inMemoryDriver(SquirrelDatabase.Schema)
