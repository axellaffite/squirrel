package app.squirrel.ui.signin

import androidx.compose.ui.text.input.PlatformImeOptions

/** A sign-in field the system's password manager fills. */
enum class CredentialField { Username, Password }

/**
 * Platform keyboard options for [field]. On iOS: the system's native text input, with the content
 * type that makes the QuickType bar offer saved passwords. Null where Compose's defaults do (Android).
 */
expect fun credentialImeOptions(field: CredentialField): PlatformImeOptions?
