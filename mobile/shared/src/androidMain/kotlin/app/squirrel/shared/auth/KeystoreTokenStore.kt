package app.squirrel.shared.auth

import android.security.keystore.KeyGenParameterSpec
import android.security.keystore.KeyProperties
import app.squirrel.shared.db.SquirrelDatabase
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import java.security.KeyStore
import javax.crypto.Cipher
import javax.crypto.KeyGenerator
import javax.crypto.SecretKey
import javax.crypto.spec.GCMParameterSpec

/**
 * The refresh token, AES-256-GCM encrypted under a key generated in the Android Keystore (no user
 * authentication, so background work can read it); the IV and ciphertext live in `secret`.
 * The key never leaves the device, so a restored copy of the database is useless.
 */
class KeystoreTokenStore(private val db: SquirrelDatabase) : TokenStore {
    /**
     * The stored token, or null when there is none. A ciphertext that can never be decrypted again
     * is deleted ([isUnrecoverableSecretFailure]); any other failure throws [TokenStoreUnavailable]
     * and keeps the row, so a later attempt can succeed.
     */
    override suspend fun load(): String? = withContext(Dispatchers.IO) {
        val row = db.secretQueries.get(SECRET_NAME).executeAsOneOrNull() ?: return@withContext null
        var keyLoaded = false
        try {
            val key = key()
            keyLoaded = true
            val cipher = Cipher.getInstance(TRANSFORMATION)
            cipher.init(Cipher.DECRYPT_MODE, key, GCMParameterSpec(TAG_BITS, row.iv))
            cipher.doFinal(row.ciphertext).decodeToString()
        } catch (e: CancellationException) {
            throw e
        } catch (e: Exception) {
            if (!isUnrecoverableSecretFailure(e, keyLoaded)) throw TokenStoreUnavailable(e)
            db.secretQueries.remove(SECRET_NAME)
            null
        }
    }

    override suspend fun save(refreshToken: String) {
        withContext(Dispatchers.IO) {
            val cipher = Cipher.getInstance(TRANSFORMATION)
            cipher.init(Cipher.ENCRYPT_MODE, key())
            val ciphertext = cipher.doFinal(refreshToken.encodeToByteArray())
            db.secretQueries.put(SECRET_NAME, cipher.iv, ciphertext)
        }
    }

    override suspend fun clear() {
        withContext(Dispatchers.IO) { db.secretQueries.remove(SECRET_NAME) }
    }

    private fun key(): SecretKey {
        val keyStore = KeyStore.getInstance(ANDROID_KEYSTORE).apply { load(null) }
        (keyStore.getEntry(KEY_ALIAS, null) as? KeyStore.SecretKeyEntry)?.let { return it.secretKey }
        val generator = KeyGenerator.getInstance(KeyProperties.KEY_ALGORITHM_AES, ANDROID_KEYSTORE)
        generator.init(
            KeyGenParameterSpec.Builder(KEY_ALIAS, KeyProperties.PURPOSE_ENCRYPT or KeyProperties.PURPOSE_DECRYPT)
                .setBlockModes(KeyProperties.BLOCK_MODE_GCM)
                .setEncryptionPaddings(KeyProperties.ENCRYPTION_PADDING_NONE)
                .setKeySize(KEY_BITS)
                .build(),
        )
        return generator.generateKey()
    }

    private companion object {
        const val ANDROID_KEYSTORE = "AndroidKeyStore"
        const val KEY_ALIAS = "squirrel-refresh"
        const val SECRET_NAME = "refresh_token"
        const val TRANSFORMATION = "AES/GCM/NoPadding"
        const val TAG_BITS = 128
        const val KEY_BITS = 256
    }
}
