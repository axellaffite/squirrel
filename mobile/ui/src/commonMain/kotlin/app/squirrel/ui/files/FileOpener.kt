package app.squirrel.ui.files

import androidx.compose.foundation.text.BasicText
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import app.squirrel.api.models.NodeDto
import app.squirrel.shared.api.SquirrelApi
import app.squirrel.shared.sync.formatBytes
import app.squirrel.theme.SquirrelTheme
import app.squirrel.theme.components.Button
import app.squirrel.theme.components.ButtonVariant
import app.squirrel.theme.components.Sheet
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.launch

data class OpenerState(
    /** Download progress (0–1) per file id. */
    val downloads: Map<String, Float> = emptyMap(),
    /** A large file waiting for "Download 1.4 GB?". */
    val confirm: NodeDto? = null,
    val message: String? = null,
) {
    val confirmText: String? get() = confirm?.let { "Download ${formatBytes(it.propertySize)}?" }
}

/**
 * Opens a file with another app, as Files does: downloads it first (asking above
 * [CONFIRM_BYTES]) and reports what became of it. For screens that list files from elsewhere
 * (search results, shared items).
 */
class FileOpener(private val api: SquirrelApi, private val handoff: FileHandoff?, private val scope: CoroutineScope) {
    private val _state = MutableStateFlow(OpenerState())
    val state: StateFlow<OpenerState> = _state.asStateFlow()

    val available: Boolean get() = handoff != null

    fun open(node: NodeDto, confirmed: Boolean = false) {
        val handoff = handoff ?: return
        if (node.id in _state.value.downloads) return
        if (!confirmed && node.propertySize > CONFIRM_BYTES) return _state.update { it.copy(confirm = node) }
        _state.update { it.copy(confirm = null, message = null, downloads = it.downloads + (node.id to 0f)) }
        scope.launch {
            val result = try {
                handoff.open(api, node.id, node.name, node.mime, share = false) { progress ->
                    _state.update { s -> if (node.id in s.downloads) s.copy(downloads = s.downloads + (node.id to progress)) else s }
                }
            } finally {
                _state.update { it.copy(downloads = it.downloads - node.id) }
            }
            _state.update { it.copy(message = handoffMessage(result, node.name)) }
        }
    }

    fun dismissConfirm() = _state.update { it.copy(confirm = null) }

    fun dismissMessage() = _state.update { it.copy(message = null) }
}

/** What to tell after a hand-off; null when the file opened. */
fun handoffMessage(result: HandoffResult, name: String): String? = when (result) {
    HandoffResult.Opened -> null
    HandoffResult.NoApp -> "No app on this phone opens $name."
    is HandoffResult.Failed -> result.message
}

/** The "Download 1.4 GB?" sheet of [opener], when one is waiting. */
@Composable
fun OpenerConfirm(opener: FileOpener) {
    val state by opener.state.collectAsStateWithLifecycle()
    val big = state.confirm ?: return
    Sheet(state.confirmText.orEmpty(), onDismiss = opener::dismissConfirm) {
        BasicText("${big.name} is large: it is downloaded to this phone before it opens.", style = SquirrelTheme.type.body)
        Button("Download", onClick = { opener.open(big, confirmed = true) }, full = true)
        Button("Cancel", onClick = opener::dismissConfirm, variant = ButtonVariant.Quiet, full = true)
    }
}
