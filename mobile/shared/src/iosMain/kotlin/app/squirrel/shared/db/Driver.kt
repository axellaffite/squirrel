package app.squirrel.shared.db

import app.cash.sqldelight.db.SqlDriver
import app.cash.sqldelight.driver.native.NativeSqliteDriver

/** The app's database in its Application Support directory. */
fun createIosDriver(): SqlDriver = NativeSqliteDriver(SquirrelDatabase.Schema, "squirrel.db")
