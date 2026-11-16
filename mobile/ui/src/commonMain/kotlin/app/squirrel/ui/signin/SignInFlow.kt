package app.squirrel.ui.signin

import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.lifecycle.viewmodel.compose.viewModel
import app.squirrel.ui.AppGraph

/** The server and sign-in screens, ending in [AppGraph.onSignedIn]. */
@Composable
fun SignInFlow(graph: AppGraph, attempt: Int, lastServer: String?) {
    val vm = viewModel(key = "sign-in-$attempt") {
        SignInViewModel(apiFor = graph::newApi, device = graph.device, lastServer = lastServer)
    }
    val state by vm.state.collectAsStateWithLifecycle()
    when (val step = state.step) {
        SignInStep.Server -> ServerScreen(state, vm::onServerChange, vm::onContinue)
        SignInStep.Credentials -> SignInScreen(state, vm::onUsernameChange, vm::onPasswordChange, vm::onSignIn, vm::onChangeServer)
        is SignInStep.SignedIn -> LaunchedEffect(step) { graph.onSignedIn(step.serverUrl, step.api, step.result) }
    }
}
