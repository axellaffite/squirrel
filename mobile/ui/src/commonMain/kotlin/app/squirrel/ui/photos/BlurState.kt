package app.squirrel.ui.photos

/**
 * Which photos show blurred: the caller's photos matching their blur filter (the server lists
 * them), unless revealed one by one or all at once until the app restarts. Until the list first
 * arrives, every photo is blurred if the filter blurred anything last time ([wasActive]), so a
 * hidden photo never flashes on opening the app.
 */
data class BlurState(
    /** Node ids the server lists as blurred; null before the first answer. */
    val ids: Set<String>? = null,
    /** Revealed one by one ("Show"), for this run of the app. */
    val revealed: Set<String> = emptySet(),
    /** "Show blurred": every one revealed, for this run of the app. */
    val revealAll: Boolean = false,
    /** The filter blurred something last time: blur everything while the list is loading. */
    val wasActive: Boolean = false,
) {
    fun isBlurred(id: String): Boolean {
        if (revealAll || id in revealed) return false
        return ids?.contains(id) ?: wasActive
    }

    /** Whether the filter blurs anything (the "Show blurred" toggle is offered only then). */
    val any: Boolean get() = !ids.isNullOrEmpty()

    fun withIds(list: List<String>): BlurState = copy(ids = list.toSet(), wasActive = list.isNotEmpty())

    fun withRevealed(id: String): BlurState = copy(revealed = revealed + id)

    /** Shows every blurred photo, or blurs them all again (forgetting the one-by-one reveals). */
    fun withRevealAll(all: Boolean): BlurState = copy(revealAll = all, revealed = emptySet())
}
