package app.squirrel.shared.hash

import kotlinx.cinterop.ExperimentalForeignApi
import kotlinx.cinterop.UByteVar
import kotlinx.cinterop.addressOf
import kotlinx.cinterop.alloc
import kotlinx.cinterop.free
import kotlinx.cinterop.nativeHeap
import kotlinx.cinterop.ptr
import kotlinx.cinterop.readBytes
import kotlinx.cinterop.allocArray
import kotlinx.cinterop.memScoped
import kotlinx.cinterop.usePinned
import platform.CoreCrypto.CC_SHA256_CTX
import platform.CoreCrypto.CC_SHA256_DIGEST_LENGTH
import platform.CoreCrypto.CC_SHA256_Final
import platform.CoreCrypto.CC_SHA256_Init
import platform.CoreCrypto.CC_SHA256_Update

@OptIn(ExperimentalForeignApi::class)
actual class Sha256 actual constructor() {
    private val context = nativeHeap.alloc<CC_SHA256_CTX>().also { CC_SHA256_Init(it.ptr) }

    actual fun update(bytes: ByteArray, offset: Int, length: Int) {
        if (length == 0) return
        bytes.usePinned { CC_SHA256_Update(context.ptr, it.addressOf(offset), length.toUInt()) }
    }

    actual fun digestHex(): String = memScoped {
        val out = allocArray<UByteVar>(CC_SHA256_DIGEST_LENGTH)
        CC_SHA256_Final(out, context.ptr)
        nativeHeap.free(context)
        out.readBytes(CC_SHA256_DIGEST_LENGTH).toHex()
    }
}
