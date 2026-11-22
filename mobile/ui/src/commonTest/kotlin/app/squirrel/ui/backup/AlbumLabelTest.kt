package app.squirrel.ui.backup

import app.squirrel.shared.settings.AlbumFilter
import app.squirrel.shared.sync.formatCount
import kotlin.test.Test
import kotlin.test.assertEquals

class AlbumLabelTest {
    private val folders = listOf(
        AlbumChoice("1", "Camera", 1_204, "DCIM/Camera"),
        AlbumChoice("2", "Screenshots", 8, "Pictures/Screenshots"),
        AlbumChoice("3", "WhatsApp Images", 30, "Android/media/com.whatsapp/WhatsApp/Media/WhatsApp Images"),
        AlbumChoice("4", "Download", 2, "Download"),
    )

    private fun label(vararg ids: String, albums: List<AlbumChoice>? = folders, kind: AlbumKind = AlbumKind.Folders) =
        albumsLabel(AlbumFilter.Selected(ids.toSet()), albums, kind)

    @Test
    fun everythingIsAllPhotos() {
        assertEquals("All photos", albumsLabel(AlbumFilter.All, folders, AlbumKind.Folders))
    }

    @Test
    fun theChosenFoldersAreNamed() {
        assertEquals("Camera", label("1"))
        assertEquals("Camera, Screenshots", label("1", "2"))
        assertEquals("Camera, Screenshots and 2 more", label("1", "2", "3", "4"))
    }

    @Test
    fun beforeTheNamesAreKnownItCounts() {
        assertEquals("1 folder", label("1", albums = null))
        assertEquals("2 albums", label("1", "2", albums = null, kind = AlbumKind.Albums))
        assertEquals("No folders", label())
    }

    @Test
    fun aFolderNoLongerOnThePhoneStillCounts() {
        assertEquals("Camera and 1 more", label("1", "removed"))
    }

    @Test
    fun underEachNameItsPathAndCount() {
        assertEquals("DCIM/Camera · ${formatCount(1_204)}", albumMeta(folders[0]))
        assertEquals("12", albumMeta(AlbumChoice("x", "Holidays", 12))) // iOS: no path
    }

    @Test
    fun androidSaysFoldersAndIosSaysAlbums() {
        assertEquals("Folders", AlbumKind.Folders.title)
        assertEquals("Albums", AlbumKind.Albums.title)
    }
}
