package app.squirrel.ui.signin

import platform.UIKit.UITextContentTypePassword
import platform.UIKit.UITextContentTypeUsername
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertNotNull

class CredentialInputTest {
    @Test
    fun theCredentialFieldsTellUIKitWhatTheyHold() {
        assertEquals(CredentialTraits(UITextContentTypeUsername, secure = false), credentialTraits(CredentialField.Username))
        assertEquals(CredentialTraits(UITextContentTypePassword, secure = true), credentialTraits(CredentialField.Password))
    }

    @Test
    fun bothFieldsGetPlatformOptions() {
        CredentialField.entries.forEach { assertNotNull(credentialImeOptions(it)) }
    }
}
