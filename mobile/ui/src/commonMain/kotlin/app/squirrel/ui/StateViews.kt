package app.squirrel.ui

import kotlinx.coroutines.ExperimentalForInheritanceCoroutinesApi
import kotlinx.coroutines.awaitCancellation
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.FlowCollector
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.combine
import kotlinx.coroutines.flow.distinctUntilChanged
import kotlinx.coroutines.flow.map

/**
 * A view of this state, computed when read: no coroutine keeps it current, so nothing outlives
 * the screen (a `stateIn(scope, Eagerly)` collector runs as long as its scope, on that scope's
 * dispatcher). For LS-DO views: the registry reads `value`, a collector gets each change.
 */
fun <T, R> StateFlow<T>.mapState(transform: (T) -> R): StateFlow<R> =
    DerivedStateFlow({ transform(value) }, map(transform))

/** [mapState] over two states. */
fun <A, B, R> combineState(a: StateFlow<A>, b: StateFlow<B>, transform: (A, B) -> R): StateFlow<R> =
    DerivedStateFlow({ transform(a.value, b.value) }, combine(a, b, transform))

@OptIn(ExperimentalForInheritanceCoroutinesApi::class)
private class DerivedStateFlow<R>(private val read: () -> R, changes: Flow<R>) : StateFlow<R> {
    private val changes = changes.distinctUntilChanged()

    override val value: R get() = read()

    override val replayCache: List<R> get() = listOf(value)

    override suspend fun collect(collector: FlowCollector<R>): Nothing {
        changes.collect(collector)
        awaitCancellation()
    }
}
