package app.squirrel.ui.files

import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.lifecycle.viewmodel.compose.viewModel
import app.squirrel.theme.SquirrelIcon
import app.squirrel.theme.components.MenuAction
import app.squirrel.ui.AppGraph
import app.squirrel.ui.photos.UNREACHABLE
import app.squirrel.ui.shell.NameSheet
import kotlinx.coroutines.launch

/**
 * The bottom bar's add menu (MASTER.md "Bottom bar"): Upload files and New folder, into the
 * folder open in Files, otherwise the top-level "Files" folder. They reuse the Files tab's own
 * view models (same keys as [FilesTab] and [FolderBrowser]), its picker and its name sheet; once
 * something is on its way, [showFiles] brings the Files tab up on that folder.
 */
@Composable
fun addMenu(graph: AppGraph, showFiles: () -> Unit): List<MenuAction> {
    val api = graph.api ?: return emptyList()
    val files = viewModel(key = "files") { FilesViewModel(api, graph.files) }
    val uploads = viewModel(key = "uploads") { UploadsViewModel(api) }
    val picker = LocalFilePicker.current
    val scope = rememberCoroutineScope()
    var naming by remember { mutableStateOf(false) }
    if (naming) {
        NameSheet("New folder", "Create folder", validate = ::nameProblem, onSubmit = { name ->
            val target = files.addTarget()
            when {
                target == null -> UNREACHABLE
                else -> files.createFolderIn(target, name).also {
                    if (it == null) {
                        files.show(target)
                        showFiles()
                    }
                }
            }
        }, onClose = { naming = false })
    }
    return buildList {
        if (picker != null) {
            add(MenuAction("Upload files", SquirrelIcon.Upload) {
                scope.launch {
                    val target = files.addTarget()
                    if (target == null) {
                        files.tell(UNREACHABLE)
                        showFiles()
                        return@launch
                    }
                    val picked = picker.pick()
                    if (picked.isEmpty()) return@launch
                    uploads.enqueue(picked, target)
                    files.show(target)
                    showFiles()
                }
            })
        }
        add(MenuAction("New folder", SquirrelIcon.FolderPlus) { naming = true })
    }
}
