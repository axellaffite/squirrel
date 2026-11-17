package app.squirrel.shared.db

import app.cash.sqldelight.db.SqlDriver
import app.cash.sqldelight.driver.native.NativeSqliteDriver

/** The app's database in its Application Support directory ([name]: another file, for tests). */
fun createIosDriver(name: String = "squirrel.db"): SqlDriver = NativeSqliteDriver(SquirrelDatabase.Schema, name)
