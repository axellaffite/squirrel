import XCTest
import SquirrelKit

/// The Kotlin Keychain store, run inside the signed host app (the Kotlin/Native test runner has no
/// Keychain entitlement on the simulator).
final class KeychainTokenStoreTests: XCTestCase {
    // Its own item: never the app's.
    private let store = KeychainTokenStore(service: "app.squirrel.test", account: "refresh_token")

    override func tearDown() async throws {
        try await store.clear()
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
}
