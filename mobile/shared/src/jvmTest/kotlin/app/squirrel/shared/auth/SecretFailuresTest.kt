package app.squirrel.shared.auth

import android.security.keystore.KeyPermanentlyInvalidatedException
import java.security.KeyStoreException
import java.security.ProviderException
import javax.crypto.AEADBadTagException
import kotlin.test.Test
import kotlin.test.assertFalse
import kotlin.test.assertTrue

class SecretFailuresTest {
    @Test
    fun anInvalidatedKeyIsUnrecoverable() {
        assertTrue(isUnrecoverableSecretFailure(KeyPermanentlyInvalidatedException(), keyLoaded = false))
        assertTrue(isUnrecoverableSecretFailure(RuntimeException(KeyPermanentlyInvalidatedException()), keyLoaded = false))
    }

    @Test
    fun aBadTagIsUnrecoverableOnlyOnceTheKeyLoaded() {
        assertTrue(isUnrecoverableSecretFailure(AEADBadTagException("tag"), keyLoaded = true))
        assertFalse(isUnrecoverableSecretFailure(AEADBadTagException("tag"), keyLoaded = false))
    }

    @Test
    fun keystoreAndProviderTroubleIsTransient() {
        assertFalse(isUnrecoverableSecretFailure(KeyStoreException("busy"), keyLoaded = false))
        assertFalse(isUnrecoverableSecretFailure(ProviderException("keystore2 died"), keyLoaded = true))
        assertFalse(isUnrecoverableSecretFailure(IllegalStateException(), keyLoaded = true))
    }
}
