package app.squirrel.ui.photos

import app.squirrel.shared.api.SquirrelApi

/** What became of a download saved to the phone. */
sealed interface SaveResult {
    /** Kept on the phone, in [place] ("Photos", "Pictures", "Movies", "Downloads"). */
    data class Saved(val place: String) : SaveResult

    /** Handed to the system's share sheet (a zip on iOS), where the person picks where it goes. */
    data object Handed : SaveResult

    data class Failed(val message: String) : SaveResult
}

/**
 * Saves downloads to the phone. Platform-specific: Android writes into MediaStore (Pictures/Squirrel,
 * Movies/Squirrel, Downloads), iOS adds to the Photos library and shares zips through the share sheet.
 */
interface PhotoSaver {
    /** Downloads a photo's or video's original and adds it to the phone's photo library. */
    suspend fun saveOriginal(api: SquirrelApi, id: String, name: String, mime: String?, onProgress: (Float) -> Unit): SaveResult

    /** Downloads the zip at [url] (`GET /api/files/archive/{ticket}`) and keeps it as [name]. */
    suspend fun saveArchive(api: SquirrelApi, url: String, name: String, onProgress: (Float) -> Unit): SaveResult
}
