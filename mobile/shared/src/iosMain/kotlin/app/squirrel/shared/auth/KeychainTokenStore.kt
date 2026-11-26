package app.squirrel.shared.auth

import kotlinx.cinterop.BetaInteropApi
import kotlinx.cinterop.ExperimentalForeignApi
import kotlinx.cinterop.alloc
import kotlinx.cinterop.memScoped
import kotlinx.cinterop.ptr
import kotlinx.cinterop.value
import platform.CoreFoundation.CFDictionaryAddValue
import platform.CoreFoundation.CFDictionaryCreateMutable
import platform.CoreFoundation.CFDictionaryGetValue
import platform.CoreFoundation.CFDictionaryRef
import platform.CoreFoundation.CFMutableDictionaryRef
import platform.CoreFoundation.CFRelease
import platform.CoreFoundation.CFRetain
import platform.CoreFoundation.CFTypeRef
import platform.CoreFoundation.CFTypeRefVar
import platform.CoreFoundation.kCFBooleanFalse
import platform.CoreFoundation.kCFBooleanTrue
import platform.CoreFoundation.kCFTypeDictionaryKeyCallBacks
import platform.CoreFoundation.kCFTypeDictionaryValueCallBacks
import platform.Foundation.CFBridgingRelease
import platform.Foundation.CFBridgingRetain
import platform.Foundation.NSData
import platform.Foundation.NSNumber
import platform.Foundation.NSString
import platform.Foundation.NSUTF8StringEncoding
import platform.Foundation.create
import platform.Foundation.dataUsingEncoding
import platform.Security.SecItemAdd
import platform.Security.SecItemCopyMatching
import platform.Security.SecItemDelete
import platform.Security.SecItemUpdate
import platform.Security.errSecItemNotFound
import platform.Security.errSecSuccess
import platform.Security.kSecAttrAccessGroup
import platform.Security.kSecAttrAccessible
import platform.Security.kSecAttrAccessibleAfterFirstUnlockThisDeviceOnly
import platform.Security.kSecAttrAccount
import platform.Security.kSecAttrService
import platform.Security.kSecAttrSynchronizable
import platform.Security.kSecClass
import platform.Security.kSecClassGenericPassword
import platform.Security.kSecMatchLimitAll
import platform.Security.kSecMatchLimit
import platform.Security.kSecMatchLimitOne
import platform.Security.kSecReturnAttributes
import platform.Security.kSecReturnData
import platform.Security.kSecValueData

/** A Keychain call failed with [status] (an `OSStatus`, e.g. -25308 while the device is locked). */
class KeychainError(val status: Int) : Exception("Keychain status $status")

/**
 * The refresh token in the Keychain: a generic password readable after the first unlock (so
 * background tasks can refresh), never in backups or on another device. A Keychain that cannot
 * be read now throws [TokenStoreUnavailable] (the session stays signed in); a missing item is null.
 */
@OptIn(ExperimentalForeignApi::class, BetaInteropApi::class)
class KeychainTokenStore(
    private val service: String = SERVICE,
    private val account: String = ACCOUNT,
    /**
     * The access group the item lives in (the App Group, so the Share extension signs in as the
     * app does); null: the app's own. A token saved before the group existed moves into it once.
     */
    private val accessGroup: String? = null,
) : TokenStore {
    override suspend fun load(): String? {
        copyMatching(kSecReturnData)?.let { return token(it) }
        if (accessGroup == null) return null
        // Saved by an older version, in the app's own group: into the shared one, once.
        val legacy = copyMatching(kSecReturnData, scoped = false)?.let(::token) ?: return null
        save(legacy)
        deleteOutsideGroup()
        return legacy
    }

    private fun token(result: CFTypeRef): String? {
        val data = CFBridgingRelease(result) as? NSData ?: return null
        return NSString.create(data, NSUTF8StringEncoding)?.toString()
    }

    /** Deletes the copies in other groups, keeping the one in [accessGroup]. */
    private fun deleteOutsideGroup() {
        val groups = mutableListOf<String>()
        val found = copyMatching(kSecReturnAttributes, scoped = false, all = true) ?: return
        try {
            @Suppress("UNCHECKED_CAST")
            val items = CFBridgingRelease(CFRetain(found)) as? List<Map<Any?, Any?>> ?: return
            items.mapNotNullTo(groups) { it[CFBridgingRelease(CFRetain(kSecAttrAccessGroup))] as? String }
        } finally {
            CFRelease(found)
        }
        groups.filter { it != accessGroup }.forEach { group -> withQuery(group = group) { SecItemDelete(it) } }
    }

    /** Updates the item in place (never delete-then-add: a failed add would lose the token). */
    override suspend fun save(refreshToken: String) {
        @Suppress("CAST_NEVER_SUCCEEDS")
        val data = (refreshToken as NSString).dataUsingEncoding(NSUTF8StringEncoding)
            ?: throw TokenStoreUnavailable(IllegalArgumentException("not UTF-8"))
        val bytes = CFBridgingRetain(data)
        try {
            var status = withQuery { query ->
                val changes = mutable()
                CFDictionaryAddValue(changes, kSecValueData, bytes)
                CFDictionaryAddValue(changes, kSecAttrAccessible, kSecAttrAccessibleAfterFirstUnlockThisDeviceOnly)
                SecItemUpdate(query, changes).also { CFRelease(changes) }
            }
            if (status == errSecItemNotFound) {
                status = withQuery { item ->
                    CFDictionaryAddValue(item, kSecValueData, bytes)
                    CFDictionaryAddValue(item, kSecAttrAccessible, kSecAttrAccessibleAfterFirstUnlockThisDeviceOnly)
                    SecItemAdd(item, null)
                }
            }
            if (status != errSecSuccess) throw TokenStoreUnavailable(KeychainError(status))
        } finally {
            CFRelease(bytes)
        }
    }

    /** Every copy, whatever its group: signing out leaves no token behind. */
    override suspend fun clear() {
        val status = withQuery(group = null) { SecItemDelete(it) }
        if (status != errSecSuccess && status != errSecItemNotFound) throw TokenStoreUnavailable(KeychainError(status))
    }

    /** Whether the item is readable only on this device, after its first unlock, and never synced (tests). */
    fun isThisDeviceAfterFirstUnlock(): Boolean {
        val attributes = copyMatching(kSecReturnAttributes) ?: return false
        try {
            @Suppress("UNCHECKED_CAST")
            val dictionary = attributes as CFDictionaryRef
            val value = CFDictionaryGetValue(dictionary, kSecAttrAccessible)
            val synced = CFDictionaryGetValue(dictionary, kSecAttrSynchronizable)
            val notSynced = synced == null || (CFBridgingRelease(CFRetain(synced)) as? NSNumber)?.boolValue == false
            return value != null && text(value) == text(kSecAttrAccessibleAfterFirstUnlockThisDeviceOnly) && notSynced
        } finally {
            CFRelease(attributes)
        }
    }

    /** A borrowed CFString as Kotlin text. */
    private fun text(ref: CFTypeRef?): String? = CFBridgingRelease(CFRetain(ref)) as? String

    /**
     * The matching item's data or attributes (retained: the caller releases it); null when missing.
     * [scoped] false: in any group the app may read; [all]: every match (an array).
     */
    private fun copyMatching(returning: CFTypeRef?, scoped: Boolean = true, all: Boolean = false): CFTypeRef? = memScoped {
        val result = alloc<CFTypeRefVar>()
        val status = withQuery(group = if (scoped) accessGroup else null) { query ->
            CFDictionaryAddValue(query, returning, kCFBooleanTrue)
            CFDictionaryAddValue(query, kSecMatchLimit, if (all) kSecMatchLimitAll else kSecMatchLimitOne)
            SecItemCopyMatching(query, result.ptr)
        }
        when (status) {
            errSecSuccess -> result.value
            errSecItemNotFound -> null
            else -> throw TokenStoreUnavailable(KeychainError(status))
        }
    }

    /**
     * Runs [block] with a fresh query for this item (class, service, account, and [group] when
     * set: by default [accessGroup]), then frees it.
     */
    private fun <T> withQuery(group: String? = accessGroup, block: (CFMutableDictionaryRef?) -> T): T {
        val query = mutable()
        val serviceRef = CFBridgingRetain(service)
        val accountRef = CFBridgingRetain(account)
        val groupRef = group?.let { CFBridgingRetain(it) }
        try {
            CFDictionaryAddValue(query, kSecClass, kSecClassGenericPassword)
            CFDictionaryAddValue(query, kSecAttrService, serviceRef)
            CFDictionaryAddValue(query, kSecAttrAccount, accountRef)
            if (groupRef != null) CFDictionaryAddValue(query, kSecAttrAccessGroup, groupRef)
            // Never in iCloud Keychain: the refresh token belongs to this device alone.
            CFDictionaryAddValue(query, kSecAttrSynchronizable, kCFBooleanFalse)
            return block(query)
        } finally {
            CFRelease(serviceRef)
            CFRelease(accountRef)
            groupRef?.let(::CFRelease)
            CFRelease(query)
        }
    }

    private fun mutable(): CFMutableDictionaryRef? =
        CFDictionaryCreateMutable(null, 0, kCFTypeDictionaryKeyCallBacks.ptr, kCFTypeDictionaryValueCallBacks.ptr)

    private companion object {
        const val SERVICE = "app.squirrel"
        const val ACCOUNT = "refresh_token"
    }
}
