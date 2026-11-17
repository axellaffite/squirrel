package app.squirrel.android.media

import android.provider.MediaStore

/**
 * A MediaStore bucket. [id] is BUCKET_ID (what the filter keeps), [name] its display name;
 * [label] adds the volume when another bucket has the same name ("Camera (SD card)").
 */
data class Album(val id: String, val name: String, val volume: String, val count: Int, val label: String = name)

/** Labels that tell same-named albums apart by where they live. */
fun labelled(albums: List<Album>): List<Album> {
    val duplicated = albums.groupingBy { it.name }.eachCount().filterValues { it > 1 }.keys
    return albums.map { album ->
        if (album.name !in duplicated) album
        else album.copy(label = "${album.name} (${if (album.volume == MediaStore.VOLUME_EXTERNAL_PRIMARY) "phone" else "SD card"})")
    }.sortedBy { it.label.lowercase() }
}
