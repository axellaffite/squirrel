package app.squirrel.shared.db

import android.content.Context
import app.cash.sqldelight.db.SqlDriver
import app.cash.sqldelight.driver.android.AndroidSqliteDriver

const val DATABASE_NAME = "squirrel.db"

/** The app's database in its private files (excluded from backups by the manifest). */
fun createAndroidDriver(context: Context): SqlDriver =
    AndroidSqliteDriver(SquirrelDatabase.Schema, context.applicationContext, DATABASE_NAME)
