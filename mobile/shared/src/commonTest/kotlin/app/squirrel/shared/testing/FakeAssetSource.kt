package app.squirrel.shared.testing

import app.squirrel.shared.db.ScannedAsset
import app.squirrel.shared.sync.AssetReader
import app.squirrel.shared.sync.AssetSource
import app.squirrel.shared.sync.MediaAccess
import app.squirrel.shared.sync.ScanResult
import app.squirrel.shared.sync.SourceFilter

/** An in-memory phone library. Tokens are change counters; it ignores the filter (the scanner applies it). */
class FakeAssetSource : AssetSource {
    private val assets = linkedMapOf<String, ScannedAsset>()
    private val readers = mutableMapOf<String, FakeReader>()
    private val changedAt = mutableMapOf<String, Int>()
    private val removedAt = mutableMapOf<String, Int>()
    private var clock = 0
    var access: MediaAccess = MediaAccess.Full
    var opened = 0
        private set
    var scans = 0
        private set
    /** Ids the scan lists but [open] cannot read (deleted between the two). */
    val unreadable = mutableSetOf<String>()

    fun put(
        id: String,
        size: Long = 1000,
        kind: String = "photo",
        album: String = "Camera",
        modifiedAt: Long = 1,
        takenAt: String? = "2024-07-14T18:30:05+02:00",
        locationStripped: Boolean = false,
    ): ScannedAsset {
        clock++
        val asset = ScannedAsset(id, kind, "$id.jpg", if (kind == "video") "video/mp4" else "image/jpeg", size, modifiedAt, takenAt, album)
        assets[id] = asset
        readers[id] = FakeReader(size, seed = id.hashCode(), modifiedAt = modifiedAt, locationStripped = locationStripped)
        changedAt[id] = clock
        removedAt.remove(id)
        return asset
    }

    fun remove(id: String) {
        clock++
        assets.remove(id)
        readers.remove(id)
        removedAt[id] = clock
    }

    fun reader(id: String): FakeReader = readers.getValue(id)

    override suspend fun changes(token: String?, filter: SourceFilter): ScanResult {
        scans++
        val since = token?.toInt()
        return if (since == null) {
            ScanResult(assets.values.toList(), removed = emptyList(), token = clock.toString(), full = true)
        } else {
            ScanResult(
                assets = assets.values.filter { changedAt.getValue(it.deviceAssetId) > since },
                removed = removedAt.filterValues { it > since }.keys.toList(),
                token = clock.toString(),
                full = false,
            )
        }
    }

    override suspend fun open(deviceAssetId: String): AssetReader? {
        opened++
        if (deviceAssetId in unreadable) return null
        return readers[deviceAssetId]
    }

    override suspend fun existing(ids: List<String>): Set<String> = ids.filter { it in assets }.toSet()

    override fun access(): MediaAccess = access

    /** Ids the engine reported as on the server, in order. */
    val backedUp = mutableListOf<String>()
    override fun backedUp(deviceAssetId: String) {
        backedUp += deviceAssetId
    }
    var fingerprint: String = "full"
    override fun accessFingerprint(): String = fingerprint
}
