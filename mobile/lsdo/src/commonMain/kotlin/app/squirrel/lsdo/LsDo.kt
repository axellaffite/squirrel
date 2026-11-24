package app.squirrel.lsdo

import kotlin.concurrent.Volatile

/**
 * Where screens announce themselves. Without a [Sink] (release builds) registering does nothing;
 * a debug build installs the runtime's registry at startup.
 */
object LsDo {
    fun interface Sink {
        /** Lists [screen] under [name]; the returned function removes it. */
        fun register(name: String, screen: LsDoScreen<*, *>): () -> Unit
    }

    @Volatile
    private var sink: Sink? = null

    val installed: Boolean get() = sink != null

    fun install(sink: Sink) {
        this.sink = sink
    }

    fun register(name: String, screen: LsDoScreen<*, *>): () -> Unit = sink?.register(name, screen) ?: {}
}
