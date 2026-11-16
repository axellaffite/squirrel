package app.squirrel.shared.auth

import kotlinx.cinterop.BetaInteropApi
import kotlinx.cinterop.ExperimentalForeignApi
import kotlinx.cinterop.alloc
import kotlinx.cinterop.memScoped
import kotlinx.cinterop.ptr
import kotlinx.cinterop.value
import platform.CoreFoundation.CFDictionaryRef
import platform.CoreFoundation.CFTypeRefVar
import platform.Foundation.CFBridgingRelease
import platform.Foundation.CFBridgingRetain
import platform.Foundation.NSData
import platform.Foundation.NSString
import platform.Foundation.NSUTF8StringEncoding
import platform.Foundation.create
import platform.Foundation.dataUsingEncoding
import platform.Security.SecItemAdd
import platform.Security.SecItemCopyMatching
import platform.Security.SecItemDelete
import platform.Security.errSecSuccess
import platform.Security.kSecAttrAccessible
import platform.Security.kSecAttrAccessibleAfterFirstUnlockThisDeviceOnly
import platform.Security.kSecAttrAccount
import platform.Security.kSecAttrService
import platform.Security.kSecClass
import platform.Security.kSecClassGenericPassword
import platform.Security.kSecMatchLimit
import platform.Security.kSecMatchLimitOne
import platform.Security.kSecReturnData
import platform.Security.kSecValueData

/**
 * The refresh token in the Keychain: a generic password readable after the first unlock (so
 * background tasks can refresh), never in backups or on another device. Built in Milestone H.
 */
@OptIn(ExperimentalForeignApi::class, BetaInteropApi::class)
class KeychainTokenStore : TokenStore {
    override suspend fun load(): String? = memScoped {
        val query = dictionary(
            kSecClass to kSecClassGenericPassword,
            kSecAttrService to SERVICE,
            kSecAttrAccount to ACCOUNT,
            kSecReturnData to true,
            kSecMatchLimit to kSecMatchLimitOne,
        )
        val result = alloc<CFTypeRefVar>()
        val status = SecItemCopyMatching(query, result.ptr)
        CFBridgingRelease(query)
        if (status != errSecSuccess) return@memScoped null
        val data = CFBridgingRelease(result.value) as? NSData ?: return@memScoped null
        NSString.create(data, NSUTF8StringEncoding)?.toString()
    }

    override suspend fun save(refreshToken: String) {
        clear()
        @Suppress("CAST_NEVER_SUCCEEDS")
        val data = (refreshToken as NSString).dataUsingEncoding(NSUTF8StringEncoding) ?: return
        val item = dictionary(
            kSecClass to kSecClassGenericPassword,
            kSecAttrService to SERVICE,
            kSecAttrAccount to ACCOUNT,
            kSecAttrAccessible to kSecAttrAccessibleAfterFirstUnlockThisDeviceOnly,
            kSecValueData to data,
        )
        SecItemAdd(item, null)
        CFBridgingRelease(item)
    }

    override suspend fun clear() {
        val query = dictionary(
            kSecClass to kSecClassGenericPassword,
            kSecAttrService to SERVICE,
            kSecAttrAccount to ACCOUNT,
        )
        SecItemDelete(query)
        CFBridgingRelease(query)
    }

    @Suppress("UNCHECKED_CAST")
    private fun dictionary(vararg pairs: Pair<Any?, Any?>): CFDictionaryRef? =
        CFBridgingRetain(mapOf(*pairs) as Map<Any?, *>) as CFDictionaryRef?

    private companion object {
        const val SERVICE = "app.squirrel"
        const val ACCOUNT = "refresh_token"
    }
}
