package app.squirrel.ui.signin

import androidx.compose.ui.ExperimentalComposeUiApi
import androidx.compose.ui.text.input.PlatformImeOptions
import platform.UIKit.UITextContentTypePassword
import platform.UIKit.UITextContentTypeUsername

/** What UIKit is told about a credential field: its content type, and whether it is secure. */
internal data class CredentialTraits(val contentType: String?, val secure: Boolean)

internal fun credentialTraits(field: CredentialField): CredentialTraits = when (field) {
    CredentialField.Username -> CredentialTraits(UITextContentTypeUsername, secure = false)
    CredentialField.Password -> CredentialTraits(UITextContentTypePassword, secure = true)
}

@OptIn(ExperimentalComposeUiApi::class)
actual fun credentialImeOptions(field: CredentialField): PlatformImeOptions? {
    val traits = credentialTraits(field)
    return PlatformImeOptions {
        usingNativeTextInput(true) // the system's text field: the QuickType bar offers saved passwords
        textContentType(traits.contentType)
        if (traits.secure) isSecureTextEntry(true)
    }
}
