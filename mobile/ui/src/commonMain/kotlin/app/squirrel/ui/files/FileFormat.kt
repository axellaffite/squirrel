package app.squirrel.ui.files

import app.squirrel.theme.SquirrelIcon
import app.squirrel.ui.photos.monthLabel
import kotlinx.datetime.TimeZone
import kotlinx.datetime.toLocalDateTime
import kotlin.time.Instant

/** The icon of a file row, from its MIME type. */
fun fileIcon(mime: String?): SquirrelIcon = when {
    mime == null -> SquirrelIcon.File
    mime.startsWith("image/") -> SquirrelIcon.Image
    mime.startsWith("video/") -> SquirrelIcon.Film
    mime.startsWith("audio/") -> SquirrelIcon.Music
    mime == "application/pdf" || mime.startsWith("text/") -> SquirrelIcon.FileText
    else -> SquirrelIcon.File
}

/** "12 March 2026", in the phone's time zone. */
fun shortDate(instant: Instant, zone: TimeZone = TimeZone.currentSystemDefault()): String {
    val date = instant.toLocalDateTime(zone).date
    return "${date.day} ${monthLabel("${date.year}-${date.month.ordinal + 1}")}"
}
