import XCTest
import SquirrelKit

/// The Kotlin Keychain store, run inside the signed host app (the Kotlin/Native test runner has no
/// Keychain entitlement on the simulator).
final class KeychainTokenStoreTests: XCTestCase {
    // Its own item: never the app's.
    private let store = KeychainTokenStore(service: "app.squirrel.test", account: "refresh_token", accessGroup: nil)
    /// The same item in the App Group, as the app and its Share extension use it.
    private let shared = KeychainTokenStore(service: "app.squirrel.test", account: "refresh_token", accessGroup: AppGroupKt.APP_GROUP)

    override func tearDown() async throws {
        try await store.clear()
        try await shared.clear()
    }

    func testStoresReadsOverwritesAndDeletes() async throws {
        try await store.clear()
        let empty = try await store.load()
        XCTAssertNil(empty)
        try await store.save(refreshToken: "sqr_first")
        let first = try await store.load()
        XCTAssertEqual(first, "sqr_first")
        try await store.save(refreshToken: "sqr_second")
        let second = try await store.load()
        XCTAssertEqual(second, "sqr_second")
        try await store.clear()
        let gone = try await store.load()
        XCTAssertNil(gone)
        try await store.clear() // nothing there: still fine
    }

    func testTheTokenStaysOnThisDeviceAfterFirstUnlockAndIsNeverSynced() async throws {
        try await store.save(refreshToken: "sqr_1")
        XCTAssertTrue(store.isThisDeviceAfterFirstUnlock())
    }

    /// A token saved before the App Group existed moves into it on first read, once.
    func testATokenFromBeforeTheAppGroupMovesIntoIt() async throws {
        try await store.save(refreshToken: "sqr_old")
        let moved = try await shared.load()
        XCTAssertEqual(moved, "sqr_old")
        try await shared.save(refreshToken: "sqr_new")
        let read = try await shared.load()
        XCTAssertEqual(read, "sqr_new")
        try await shared.clear()
        let gone = try await store.load()
        XCTAssertNil(gone) // signing out leaves no copy in any group
    }
}
