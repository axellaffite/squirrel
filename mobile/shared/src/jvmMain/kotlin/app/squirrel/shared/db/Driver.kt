package app.squirrel.shared.db

import app.cash.sqldelight.db.SqlDriver
import app.cash.sqldelight.driver.jdbc.sqlite.JdbcSqliteDriver
import java.util.Properties

/** A file-backed database on the JVM (tools and tests); [path] null keeps it in memory. */
fun createJvmDriver(path: String? = null): SqlDriver =
    JdbcSqliteDriver(
        url = path?.let { "jdbc:sqlite:$it" } ?: JdbcSqliteDriver.IN_MEMORY,
        properties = Properties(),
        schema = SquirrelDatabase.Schema,
    )
