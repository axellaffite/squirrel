package app.squirrel.ui.files

import app.squirrel.ui.mapState
import androidx.compose.runtime.staticCompositionLocalOf
import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import app.squirrel.lsdo.Doc
import app.squirrel.lsdo.LsDoScreen
import app.squirrel.lsdo.lsdoSpec
import app.squirrel.shared.api.ApiError
import app.squirrel.shared.api.SquirrelApi
import app.squirrel.shared.sync.AssetReader
import app.squirrel.shared.sync.FileUpload
import app.squirrel.ui.photos.UNREACHABLE
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.channels.Channel
import kotlinx.coroutines.flow.MutableSharedFlow
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.SharedFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asSharedFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.launch
import kotlinx.serialization.SerialName
import kotlinx.serialization.Serializable

/** A file chosen in the system picker: its name, and its bytes once opened (null: unreadable). */
class PickedFile(val name: String, val open: suspend () -> AssetReader?, val release: () -> Unit = {})

/** The system's file (or photo) picker; null where none is wired (tests, previews). */
interface FilePicker {
    /** The files the user chose; empty when they cancelled. */
    suspend fun pick(): List<PickedFile>
}

val LocalFilePicker = staticCompositionLocalOf<FilePicker?> { null }

/** One upload as the folder page shows it. */
@Serializable
data class UploadItem(
    @Doc("The upload's id: what dismiss takes") val key: Int,
    val name: String,
    @Doc("The folder it goes into: a folder page shows the uploads whose parentId is its folderId") val parentId: String,
    @Doc("0 to 1") val progress: Float = 0f,
    @Doc("Why it failed; null while it waits or uploads") val error: String? = null,
) {
    val failed: Boolean get() = error != null
}

@Serializable
data class UploadsState(
    @Doc("Files on their way up, in order, and the failed ones until dismissed; a finished one leaves the list and its folder refreshes")
    val items: List<UploadItem> = emptyList(),
    @Doc("Uploads waiting or running") val uploading: Int = 0,
    @Doc("Failed uploads still listed") val failed: Int = 0,
)

/** What an agent can do with uploads (LS-DO). Adding files is left out: it needs the system picker. */
@Serializable
sealed interface UploadsIntent {
    @Serializable @SerialName("dismiss") @Doc("Hide a failed upload's notice by its key in items, as its Dismiss button does. Only failed ones (error set)")
    data class Dismiss(val key: Int) : UploadsIntent
}

private fun uploadsState(items: List<UploadItem>) = UploadsState(items, uploading = items.count { !it.failed }, failed = items.count { it.failed })

/**
 * Uploads from the phone into folders, one file at a time, through the backup engine's upload
 * pieces ([FileUpload]). Lives with the session, so a folder left meanwhile keeps uploading.
 */
class UploadsViewModel(api: SquirrelApi, private val upload: FileUpload = FileUpload(api)) : ViewModel(), LsDoScreen<UploadsState, UploadsIntent> {
    private val _items = MutableStateFlow<List<UploadItem>>(emptyList())
    val items: StateFlow<List<UploadItem>> = _items.asStateFlow()

    override val lsdo = lsdoSpec<UploadsState, UploadsIntent>()

    /** The uploads as LS-DO shows them, with their counts. */
    override val state: StateFlow<UploadsState> = items.mapState(::uploadsState)

    override suspend fun perform(intent: UploadsIntent): String? = when (intent) {
        is UploadsIntent.Dismiss -> {
            val item = _items.value.firstOrNull { it.key == intent.key }
            when {
                item == null -> "No upload ${intent.key} in the list."
                !item.failed -> "${item.name} is still uploading: only a failed upload can be dismissed."
                else -> null.also { dismiss(item.key) }
            }
        }
    }

    override fun availableIntents(state: UploadsState): Set<String> = if (state.failed > 0) setOf("dismiss") else emptySet()

    /** Never: uploads run as long as the files take, so a do does not wait for them; ls shows how far they got. */
    override fun isBusy(state: UploadsState): Boolean = false

    private val _finished = MutableSharedFlow<String>(extraBufferCapacity = FINISHED_BUFFER)

    /** A folder whose file just arrived: its page refreshes. */
    val finished: SharedFlow<String> = _finished.asSharedFlow()

    private val queue = Channel<Pair<UploadItem, PickedFile>>(Channel.UNLIMITED)
    private var nextKey = 0

    init {
        viewModelScope.launch {
            for ((item, file) in queue) run(item, file)
        }
    }

    fun enqueue(files: List<PickedFile>, parentId: String) {
        files.forEach { file ->
            val item = UploadItem(nextKey++, file.name, parentId)
            _items.update { it + item }
            queue.trySend(item to file)
        }
    }

    /** Hides a failed upload's notice. */
    fun dismiss(key: Int) = _items.update { list -> list.filterNot { it.key == key } }

    private suspend fun run(item: UploadItem, file: PickedFile) {
        val error = try {
            val reader = file.open() ?: return fail(item, "It could not be read on this phone.")
            reader.use { upload.upload(item.parentId, file.name, it) { p -> progress(item.key, p) } }
            null
        } catch (e: CancellationException) {
            throw e
        } catch (e: ApiError) {
            if (e.fromServer) e.message ?: UNREACHABLE else UNREACHABLE
        } catch (_: Exception) {
            UNREACHABLE
        } finally {
            file.release()
        }
        if (error != null) return fail(item, error)
        _items.update { list -> list.filterNot { it.key == item.key } }
        _finished.tryEmit(item.parentId)
    }

    private fun progress(key: Int, value: Float) = _items.update { list -> list.map { if (it.key == key) it.copy(progress = value) else it } }

    private fun fail(item: UploadItem, message: String) =
        _items.update { list -> list.map { if (it.key == item.key) it.copy(error = "Uploading ${item.name} failed: $message") else it } }

    private companion object {
        const val FINISHED_BUFFER = 16
    }
}
