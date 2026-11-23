package app.squirrel.ui.files

/** The server's NodeName limit, in UTF-8 bytes. */
private const val MAX_NAME_BYTES = 255

/** The server's AlbumName limit, in characters. */
const val ALBUM_NAME_MAX = 100


/** Mirrors the server's file and folder name rules closely enough for instant feedback. */
fun nameProblem(raw: String): String? {
    val name = raw.trim()
    return when {
        name.isEmpty() -> "Enter a name."
        name == "." || name == ".." -> "This name is reserved."
        '/' in name -> "Names cannot contain \"/\"."
        name.encodeToByteArray().size > MAX_NAME_BYTES -> "This name is too long."
        else -> null
    }
}

/** Mirrors the server's album name rules: 1–100 characters, no invisible or control character. */
fun albumNameProblem(raw: String): String? {
    val name = raw.trim()
    return when {
        name.isEmpty() -> "Enter a name."
        name.length > ALBUM_NAME_MAX -> "Album names have at most $ALBUM_NAME_MAX characters."
        name.any { it.category == CharCategory.CONTROL || it.category == CharCategory.FORMAT } -> "Names cannot contain invisible or control characters."
        else -> null
    }
}
