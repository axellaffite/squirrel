package app.squirrel.shared.db

import app.cash.sqldelight.db.SqlDriver
import app.cash.sqldelight.driver.native.NativeSqliteDriver

/**
 * The app's database ([name]: another file, for tests) in [directory]; null: the process's own
 * Application Support (SQLiter's default, `Library/Application Support/databases`).
 */
fun createIosDriver(name: String = "squirrel.db", directory: String? = null): SqlDriver =
    NativeSqliteDriver(SquirrelDatabase.Schema, name, onConfiguration = { config ->
        config.copy(extendedConfig = config.extendedConfig.copy(basePath = directory))
    })
