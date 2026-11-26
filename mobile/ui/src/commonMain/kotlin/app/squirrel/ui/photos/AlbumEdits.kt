package app.squirrel.ui.photos

import app.squirrel.api.models.AlbumDto
import app.squirrel.api.models.AlbumItemsRequest
import app.squirrel.api.models.CreateAlbumRequest
import app.squirrel.api.models.SharePermission
import app.squirrel.api.models.SharedItemDto
import app.squirrel.api.models.UpdateAlbumRequest
import app.squirrel.shared.api.SquirrelApi
import app.squirrel.shared.api.bodyOrThrow
import app.squirrel.ui.attempt
import app.squirrel.ui.explore.photoCount
import app.squirrel.ui.sharing.Outcome
import app.squirrel.ui.sharing.call

/** The API takes at most this many photos per add or remove request. */
const val MAX_ITEMS_PER_REQUEST = 500

/** "Added 3 photos to Holidays." (or why nothing changed). */
fun addedMessage(changed: Long, requested: Int, album: String): String = when {
    changed == 0L && requested == 1 -> "This photo is already in $album."
    changed == 0L -> "These photos are already in $album."
    else -> "Added ${photoCount(changed)} to $album."
}

/** "Removed 2 photos from the album.", or what to say when none were (others' photos stay). */
fun removedMessage(removed: Long): String =
    if (removed == 0L) "Nothing was removed: you can only remove the photos you added." else "Removed ${photoCount(removed)} from the album."

/**
 * The albums photos can be added to: one's own and those shared for writing, by name. Dynamic
 * albums take their photos from their rules, never by hand: they are left out.
 */
fun writableAlbums(own: List<AlbumDto>, shared: List<SharedItemDto>): List<AlbumDto> {
    val ids = own.map { it.id }.toSet()
    val extra = shared.filter { it.permission == SharePermission.write }.mapNotNull { it.album }.filter { it.id !in ids }
    return (own + extra).filterNot { it.dynamic }.sortedWith(compareBy(String.CASE_INSENSITIVE_ORDER) { it.name })
}

/** What the album picker offers; [ok] false: one's own albums could not be listed. */
class PickerAlbums(val albums: List<AlbumDto>, val ok: Boolean)

suspend fun pickerAlbums(api: SquirrelApi): PickerAlbums {
    val own = attempt { api.albums.listAlbums().bodyOrThrow() }
    val shared = attempt { api.sharing.sharedWithMe().bodyOrThrow() }.orEmpty()
    return PickerAlbums(writableAlbums(own.orEmpty(), shared), own != null)
}

/**
 * Adds photos to an album in request-sized batches, safe to retry: an album created for them is
 * kept (never created twice) and batches already sent are not sent again.
 */
class AlbumAdder(private val api: SquirrelApi, ids: List<String>) {
    private val batches = ids.chunked(MAX_ITEMS_PER_REQUEST)
    private val requested = ids.size
    private var target: Pair<String, String>? = null
    private var sent = 0
    private var changed = 0L

    /** Into an existing album. */
    suspend fun into(album: AlbumDto): Outcome<String> {
        if (target?.first != album.id) start(album.id, album.name)
        return rest()
    }

    /** Into a new album named [name], created with the first batch. */
    suspend fun intoNew(name: String): Outcome<String> {
        if (target == null) {
            val created = call { api.albums.createAlbum(CreateAlbumRequest(name = name, nodeIds = batches.firstOrNull().orEmpty())).bodyOrThrow() }
            if (created !is Outcome.Ok) return created as Outcome.Failed
            start(created.value.id, created.value.name)
            sent = 1
            changed = created.value.itemCount
        }
        return rest()
    }

    private fun start(id: String, name: String) {
        target = id to name
        sent = 0
        changed = 0
    }

    private suspend fun rest(): Outcome<String> {
        val (id, name) = target ?: return Outcome.Failed("No album chosen.")
        while (sent < batches.size) {
            val result = call { api.albums.addAlbumItems(id, AlbumItemsRequest(batches[sent])).bodyOrThrow() }
            if (result !is Outcome.Ok) return result as Outcome.Failed
            changed += result.value.changed
            sent += 1
        }
        return Outcome.Ok(addedMessage(changed, requested, name))
    }
}

/** Takes photos out of an album, a batch at a time: what was removed, or the error. */
suspend fun removeFromAlbum(api: SquirrelApi, albumId: String, ids: List<String>): Outcome<Long> {
    var removed = 0L
    for (batch in ids.chunked(MAX_ITEMS_PER_REQUEST)) {
        when (val result = call { api.albums.removeAlbumItems(albumId, AlbumItemsRequest(batch)).bodyOrThrow() }) {
            is Outcome.Ok -> removed += result.value.changed
            is Outcome.Failed -> return result
        }
    }
    return Outcome.Ok(removed)
}

suspend fun renameAlbum(api: SquirrelApi, id: String, name: String): Outcome<AlbumDto> =
    call { api.albums.updateAlbum(id, UpdateAlbumRequest(name = name)).bodyOrThrow() }

suspend fun setAlbumCover(api: SquirrelApi, id: String, photoId: String): Outcome<AlbumDto> =
    call { api.albums.updateAlbum(id, UpdateAlbumRequest(coverNodeId = photoId)).bodyOrThrow() }

/** What a dynamic album's page says instead of its photos' add and remove. */
const val DYNAMIC_EMPTY_LINE = "No photo matches its rules yet. Photos that match show up here by themselves."

suspend fun createAlbum(api: SquirrelApi, name: String): Outcome<AlbumDto> =
    call { api.albums.createAlbum(CreateAlbumRequest(name = name)).bodyOrThrow() }
