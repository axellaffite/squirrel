package app.squirrel.shared.auth

import kotlinx.io.IOException

/** The token store cannot be read right now (keystore busy, provider error): try again later. */
class TokenStoreUnavailable(cause: Throwable) : IOException("The token store is unavailable", cause)

private const val KEY_INVALIDATED = "android.security.keystore.KeyPermanentlyInvalidatedException"
private const val AEAD_BAD_TAG = "javax.crypto.AEADBadTagException"

/**
 * Whether a failure to decrypt the stored refresh token means it can never be read again, so the
 * row should be deleted: the key was permanently invalidated, or the key loaded but the
 * ciphertext does not authenticate under it (a restored database, a regenerated key). Every
 * other failure, KeyStoreException and ProviderException included, is transient.
 */
fun isUnrecoverableSecretFailure(error: Throwable, keyLoaded: Boolean): Boolean {
    val names = generateSequence(error) { it.cause }.take(8).map { it::class.qualifiedName }.toList()
    return KEY_INVALIDATED in names || (keyLoaded && AEAD_BAD_TAG in names)
}
