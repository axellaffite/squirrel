package app.squirrel.shared.ios

import kotlinx.cinterop.ExperimentalForeignApi
import platform.Foundation.NSApplicationSupportDirectory
import platform.Foundation.NSFileManager
import platform.Foundation.NSSearchPathForDirectoriesInDomains
import platform.Foundation.NSUserDomainMask

/**
 * The App Group the app and its Share extension both belong to: the Keychain access group of the
 * refresh token and the home of the database, so the extension is signed in as the app is.
 */
const val APP_GROUP = "group.app.squirrel.ios"

/** Where SQLiter keeps databases, under a `Library/Application Support`. */
private const val DATABASES = "databases"

/** The App Group's database directory, created when missing; null without the App Group entitlement. */
@OptIn(ExperimentalForeignApi::class)
fun groupDatabaseDirectory(): String? {
    val manager = NSFileManager.defaultManager
    val container = manager.containerURLForSecurityApplicationGroupIdentifier(APP_GROUP)?.path ?: return null
    val directory = "$container/Library/Application Support/$DATABASES"
    if (!manager.fileExistsAtPath(directory) && !manager.createDirectoryAtPath(directory, true, null, null)) return null
    return directory
}

/**
 * Moves the database [name] (and its WAL files) from the app's own Application Support into
 * [directory], once: an install from before the App Group keeps its settings and queue. Nothing
 * when [directory] already has one, or there is nothing to move.
 */
@OptIn(ExperimentalForeignApi::class)
fun adoptDatabase(name: String, directory: String) {
    val manager = NSFileManager.defaultManager
    if (manager.fileExistsAtPath("$directory/$name")) return
    @Suppress("UNCHECKED_CAST")
    val support = (NSSearchPathForDirectoriesInDomains(NSApplicationSupportDirectory, NSUserDomainMask, true) as List<String>).firstOrNull() ?: return
    val old = "$support/$DATABASES"
    if (old == directory || !manager.fileExistsAtPath("$old/$name")) return
    listOf(name, "$name-wal", "$name-shm").forEach { file ->
        if (manager.fileExistsAtPath("$old/$file")) manager.moveItemAtPath("$old/$file", "$directory/$file", null)
    }
}
