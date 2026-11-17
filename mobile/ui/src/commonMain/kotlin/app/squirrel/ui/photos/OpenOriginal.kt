package app.squirrel.ui.photos

import app.squirrel.api.models.AssetSummary
import app.squirrel.shared.api.SquirrelApi
import app.squirrel.shared.api.bodyOrThrow
import app.squirrel.shared.sync.formatBytes
import app.squirrel.ui.attempt
import app.squirrel.ui.files.CONFIRM_BYTES
import app.squirrel.ui.files.FileHandoff
import app.squirrel.ui.files.HandoffResult
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.launch

data class OriginalState(
    /** Download progress (0–1) while the original comes down. */
    val progress: Float? = null,
    /** A large original waiting for "Download 1.4 GB?". */
    val confirm: Pair<AssetSummary, Long>? = null,
    val message: String? = null,
) {
    val confirmText: String? get() = confirm?.let { "Download ${formatBytes(it.second)}?" }
}

/** The viewer's "Open in another app": asks first above [CONFIRM_BYTES], like Files does. */
class OpenOriginal(private val api: SquirrelApi, private val files: FileHandoff, private val scope: CoroutineScope) {
    private val _state = MutableStateFlow(OriginalState())
    val state: StateFlow<OriginalState> = _state.asStateFlow()

    fun request(item: AssetSummary) {
        if (_state.value.progress != null) return
        _state.update { it.copy(progress = 0f, message = null) }
        scope.launch {
            val size = attempt { api.photos.getPhoto(item.id).bodyOrThrow().propertySize }
            when {
                size == null -> _state.update { it.copy(progress = null, message = UNREACHABLE) }
                size > CONFIRM_BYTES -> _state.update { it.copy(progress = null, confirm = item to size) }
                else -> download(item)
            }
        }
    }

    fun confirm() {
        val (item, _) = _state.value.confirm ?: return
        _state.update { it.copy(confirm = null, progress = 0f) }
        scope.launch { download(item) }
    }

    fun dismiss() = _state.update { it.copy(confirm = null) }

    private suspend fun download(item: AssetSummary) {
        val result = try {
            files.open(api, item.id, item.name, item.mime, share = false) { p -> _state.update { it.copy(progress = p) } }
        } finally {
            _state.update { it.copy(progress = null) }
        }
        _state.update {
            it.copy(
                message = when (result) {
                    HandoffResult.Opened -> null
                    HandoffResult.NoApp -> "No app on this phone opens this file."
                    is HandoffResult.Failed -> result.message
                },
            )
        }
    }
}
