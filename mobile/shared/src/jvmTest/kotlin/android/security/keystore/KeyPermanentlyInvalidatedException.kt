package android.security.keystore

import java.security.InvalidKeyException

/** Stand-in for the Android class, so the JVM test can raise it by name. */
class KeyPermanentlyInvalidatedException : InvalidKeyException("Key permanently invalidated")
