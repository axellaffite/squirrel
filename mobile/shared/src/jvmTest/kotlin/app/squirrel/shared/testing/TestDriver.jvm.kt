package app.squirrel.shared.testing

import app.cash.sqldelight.db.SqlDriver
import app.cash.sqldelight.driver.jdbc.sqlite.JdbcSqliteDriver
import app.squirrel.shared.db.SquirrelDatabase

actual fun testDriver(): SqlDriver =
    JdbcSqliteDriver(JdbcSqliteDriver.IN_MEMORY).also { SquirrelDatabase.Schema.create(it) }
