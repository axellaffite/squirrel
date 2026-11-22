package app.squirrel.android.media

import android.provider.MediaStore
import app.squirrel.ui.backup.AlbumChoice

/**
 * A MediaStore bucket, i.e. a folder. [id] is BUCKET_ID (what the filter keeps), [name] its
 * display name, [path] its RELATIVE_PATH ("DCIM/Camera/"); [label] adds the volume when another
 * bucket has the same name ("Camera (SD card)").
 */
data class Album(
    val id: String,
    val name: String,
    val volume: String,
    val count: Int,
    val label: String = name,
    val path: String? = null,
)

/**
 * The folder as the picker shows it: "Camera" over "DCIM/Camera", so the camera folder is
 * recognisable even where the phone's gallery calls it something else ("Appareil photo").
 */
fun Album.choice(): AlbumChoice = AlbumChoice(id, label, count, folderPath(path))

/** "DCIM/Camera/" → "DCIM/Camera"; null when MediaStore has no path. */
fun folderPath(relativePath: String?): String? = relativePath?.trim()?.trimEnd('/')?.takeIf { it.isNotEmpty() }

/** Labels that tell same-named albums apart by where they live. */
fun labelled(albums: List<Album>): List<Album> {
    val duplicated = albums.groupingBy { it.name }.eachCount().filterValues { it > 1 }.keys
    return albums.map { album ->
        if (album.name !in duplicated) album
        else album.copy(label = "${album.name} (${if (album.volume == MediaStore.VOLUME_EXTERNAL_PRIMARY) "phone" else "SD card"})")
    }.sortedBy { it.label.lowercase() }
}
