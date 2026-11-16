package app.squirrel.ui.signin

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.text.BasicText
import androidx.compose.foundation.text.KeyboardActions
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.text.input.ImeAction
import androidx.compose.ui.text.input.KeyboardCapitalization
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.text.style.TextOverflow
import app.squirrel.theme.SquirrelTheme
import app.squirrel.theme.Tokens
import app.squirrel.theme.components.Alert
import app.squirrel.theme.components.AlertTone
import app.squirrel.theme.components.Button
import app.squirrel.theme.components.ButtonSize
import app.squirrel.theme.components.ButtonVariant
import app.squirrel.theme.components.TextField

/** Step 1: where the user's Squirrel answers. */
@Composable
fun ServerScreen(state: SignInState, onServerChange: (String) -> Unit, onContinue: () -> Unit) {
    AuthLayout(title = "Connect to your Squirrel") {
        TextField(
            value = state.server,
            onValueChange = onServerChange,
            label = "Your Squirrel's address",
            error = state.serverError,
            enabled = !state.busy,
            keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Uri, imeAction = ImeAction.Go, autoCorrectEnabled = false),
            keyboardActions = KeyboardActions(onGo = { onContinue() }),
        )
        state.alert?.let { Alert(it, tone = AlertTone.Error) }
        Button(
            text = if (state.busy) "Connecting…" else "Continue",
            onClick = onContinue,
            enabled = !state.busy,
            full = true,
            size = ButtonSize.Lg,
        )
    }
}

/** Step 2: username and password. */
@Composable
fun SignInScreen(
    state: SignInState,
    onUsernameChange: (String) -> Unit,
    onPasswordChange: (String) -> Unit,
    onSignIn: () -> Unit,
    onChangeServer: () -> Unit,
) {
    AuthLayout(title = "Sign in") {
        Row(
            modifier = Modifier.fillMaxWidth(),
            horizontalArrangement = Arrangement.spacedBy(Tokens.space2),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            BasicText(
                state.server,
                style = SquirrelTheme.type.caption,
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
                modifier = Modifier.weight(1f),
            )
            Button("Change", onClick = onChangeServer, variant = ButtonVariant.Quiet, size = ButtonSize.Sm, enabled = !state.busy)
        }
        if (state.cleartext) {
            Alert("Not encrypted by the app: use this address only on your home network or VPN.")
        }
        TextField(
            value = state.username,
            onValueChange = onUsernameChange,
            label = "Username",
            error = state.usernameError,
            enabled = !state.busy,
            keyboardOptions = KeyboardOptions(
                capitalization = KeyboardCapitalization.None,
                autoCorrectEnabled = false,
                imeAction = ImeAction.Next,
            ),
        )
        TextField(
            value = state.password,
            onValueChange = onPasswordChange,
            label = "Password",
            error = state.passwordError,
            password = true,
            enabled = !state.busy,
            keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Password, imeAction = ImeAction.Done),
            keyboardActions = KeyboardActions(onDone = { onSignIn() }),
        )
        state.alert?.let { Alert(it, tone = AlertTone.Error) }
        Button(
            text = if (state.busy) "Signing in…" else "Sign in",
            onClick = onSignIn,
            enabled = !state.busy,
            full = true,
            size = ButtonSize.Lg,
        )
        BasicText(
            "Have an invitation? Open the link in your browser, then sign in here.",
            style = SquirrelTheme.type.caption,
        )
    }
}
