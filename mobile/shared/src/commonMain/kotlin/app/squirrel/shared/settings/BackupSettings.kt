package app.squirrel.shared.settings

import app.cash.sqldelight.coroutines.asFlow
import app.cash.sqldelight.coroutines.mapToList
import app.squirrel.shared.db.SquirrelDatabase
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.IO
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.distinctUntilChanged
import kotlinx.coroutines.flow.map
import kotlinx.serialization.builtins.SetSerializer
import kotlinx.serialization.builtins.serializer
import kotlinx.serialization.json.Json

/** Which albums are backed up. */
sealed interface AlbumFilter {
    data object All : AlbumFilter
    data class Selected(val names: Set<String>) : AlbumFilter
}

/** The backup options (Backup › Settings). */
data class BackupSettings(
    val wifiOnly: Boolean = true,
    val chargingOnly: Boolean = false,
    val includeVideos: Boolean = true,
    val albums: AlbumFilter = AlbumFilter.All,
)

private object Keys {
    const val WIFI_ONLY = "backup.wifi_only"
    const val CHARGING_ONLY = "backup.charging_only"
    const val INCLUDE_VIDEOS = "backup.include_videos"
    const val ALBUMS = "backup.albums"
    const val SERVER_URL = "server.url"
    val BACKUP = setOf(WIFI_ONLY, CHARGING_ONLY, INCLUDE_VIDEOS, ALBUMS)
}

private val albumNames = SetSerializer(String.serializer())

/** Settings kept in the `setting` table. */
class SettingsStore(private val db: SquirrelDatabase) {
    private val q = db.settingQueries

    fun backup(): BackupSettings = decode(q.all().executeAsList().associate { it.key to it.value_ })

    fun backupFlow(): Flow<BackupSettings> =
        q.all().asFlow().mapToList(Dispatchers.IO)
            .map { rows -> decode(rows.associate { it.key to it.value_ }) }
            .distinctUntilChanged()

    fun setBackup(settings: BackupSettings) = db.transaction {
        q.put(Keys.WIFI_ONLY, settings.wifiOnly.toString())
        q.put(Keys.CHARGING_ONLY, settings.chargingOnly.toString())
        q.put(Keys.INCLUDE_VIDEOS, settings.includeVideos.toString())
        q.put(
            Keys.ALBUMS,
            when (val albums = settings.albums) {
                AlbumFilter.All -> "*"
                is AlbumFilter.Selected -> Json.encodeToString(albumNames, albums.names)
            },
        )
    }

    /** The address of the user's Squirrel, normalised (`https://host`, no trailing slash). */
    fun serverUrl(): String? = q.get(Keys.SERVER_URL).executeAsOneOrNull()

    fun setServerUrl(url: String?) {
        if (url == null) q.remove(Keys.SERVER_URL) else q.put(Keys.SERVER_URL, url)
    }

    private fun decode(values: Map<String, String>): BackupSettings {
        val defaults = BackupSettings()
        if (values.keys.none { it in Keys.BACKUP }) return defaults
        return BackupSettings(
            wifiOnly = values[Keys.WIFI_ONLY]?.toBooleanStrictOrNull() ?: defaults.wifiOnly,
            chargingOnly = values[Keys.CHARGING_ONLY]?.toBooleanStrictOrNull() ?: defaults.chargingOnly,
            includeVideos = values[Keys.INCLUDE_VIDEOS]?.toBooleanStrictOrNull() ?: defaults.includeVideos,
            albums = when (val albums = values[Keys.ALBUMS]) {
                null, "*" -> AlbumFilter.All
                else -> AlbumFilter.Selected(Json.decodeFromString(albumNames, albums))
            },
        )
    }
}
