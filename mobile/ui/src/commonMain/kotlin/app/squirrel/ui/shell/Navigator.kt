package app.squirrel.ui.shell

import androidx.lifecycle.ViewModel
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.update

/**
 * One tab's pages, as a stack: [root] at the bottom, the page shown on top. A view model, so it
 * lives with the session (rotation and tab switches keep it) and goes at sign-out.
 */
class Navigator<R : Any>(private val root: R) : ViewModel() {
    private val _stack = MutableStateFlow(listOf(root))
    val stack: StateFlow<List<R>> = _stack.asStateFlow()

    val top: R get() = _stack.value.last()

    fun push(route: R) = _stack.update { it + route }

    /** Back one page; false at the root (nothing to go back to). */
    fun pop(): Boolean {
        if (_stack.value.size <= 1) return false
        _stack.update { it.dropLast(1) }
        return true
    }

    /** Replaces the page on top (the root stays a root). */
    fun replace(route: R) = _stack.update { if (it.size <= 1) listOf(route) else it.dropLast(1) + route }

    fun reset() = _stack.update { listOf(root) }
}
