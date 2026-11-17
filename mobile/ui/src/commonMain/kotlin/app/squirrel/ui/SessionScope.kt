package app.squirrel.ui

import androidx.lifecycle.ViewModelStore
import androidx.lifecycle.ViewModelStoreOwner

/**
 * The view models of one signed-in session. Signing out clears it, so another account (even on
 * the same server) never sees the previous one's photos or files, nor its closed client.
 */
class SessionScope : ViewModelStoreOwner {
    override val viewModelStore = ViewModelStore()

    fun close() = viewModelStore.clear()
}
