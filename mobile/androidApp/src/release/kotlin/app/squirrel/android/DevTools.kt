package app.squirrel.android

/** Release builds carry no developer tools; the debug twin starts the LS-DO server. */
internal object DevTools {
    fun start() = Unit
}
