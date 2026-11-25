package app.squirrel.ui.shell

import io.github.axellaffite.lsdo.Doc
import io.github.axellaffite.lsdo.LsDoScreen
import io.github.axellaffite.lsdo.lsdoSpec
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.serialization.SerialName
import kotlinx.serialization.Serializable

/** The bottom bar's sections, as an agent names them. */
@Serializable
enum class AppTab {
    @SerialName("photos") Photos,
    @SerialName("files") Files,
    @SerialName("shared") Shared,
    @SerialName("backup") Backup,
}

@Serializable
data class AppState(
    @Doc("The section shown") val tab: AppTab,
    @Doc("Every section of the bottom bar") val tabs: List<AppTab> = AppTab.entries,
)

/** What an agent can do anywhere once signed in (LS-DO): the navigation the bottom bar offers. */
@Serializable
sealed interface AppIntent {
    @Serializable @SerialName("go_to") @Doc("Show a section, as tapping it in the bottom bar does")
    data class GoTo(val tab: AppTab) : AppIntent
}

/**
 * The shell as an LS-DO screen ("app"). The tab stays the shell's own saveable Compose state:
 * [select] sets it exactly as the bottom bar does, and [shown] mirrors it back here.
 */
class ShellLsDo(initial: AppTab, private val select: (AppTab) -> Unit) : LsDoScreen<AppState, AppIntent> {
    override val lsdo = lsdoSpec<AppState, AppIntent>()

    private val _state = MutableStateFlow(AppState(initial))
    override val state: StateFlow<AppState> = _state.asStateFlow()

    override suspend fun perform(intent: AppIntent): String? = when (intent) {
        is AppIntent.GoTo -> {
            select(intent.tab)
            null
        }
    }

    /** The shell recomposed with [tab] shown. */
    fun shown(tab: AppTab) {
        _state.value = AppState(tab)
    }
}
