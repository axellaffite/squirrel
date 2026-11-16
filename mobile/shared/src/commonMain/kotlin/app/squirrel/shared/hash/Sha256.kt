package app.squirrel.shared.hash

/** Streaming SHA-256 on the platform's implementation (JCA, CommonCrypto). */
expect class Sha256() {
    fun update(bytes: ByteArray, offset: Int = 0, length: Int = bytes.size)

    /** Lowercase hex, 64 characters. The instance cannot be reused afterwards. */
    fun digestHex(): String
}

internal fun ByteArray.toHex(): String = joinToString("") { (it.toInt() and 0xff).toString(16).padStart(2, '0') }
