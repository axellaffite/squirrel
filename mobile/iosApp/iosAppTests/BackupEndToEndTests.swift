import Photos
import XCTest
import SquirrelKit

/// Backs up the simulator's photos to a running e2e server (`mobile/scripts/mobile-e2e-server.sh`)
/// through the Kotlin graph, then checks the server's folders. Skipped unless `SQUIRREL_SERVER` is set.
final class BackupEndToEndTests: XCTestCase {
    func testThePhotosReachTheServer() async throws {
        let env = ProcessInfo.processInfo.environment
        guard let server = env["SQUIRREL_SERVER"], server.hasPrefix("http") else {
            throw XCTSkip("Set SQUIRREL_SERVER to a running e2e server")
        }
        guard PHPhotoLibrary.authorizationStatus(for: .readWrite) == .authorized else {
            throw XCTSkip("Grant photo access to app.squirrel.ios first")
        }
        let images = PHAsset.fetchAssets(with: .image, options: nil).count
        XCTAssertGreaterThan(images, 0, "add photos with simctl addmedia first")
        let user = env["SQUIRREL_USER"] ?? "phone"
        let password = env["SQUIRREL_PASSWORD"] ?? "phone-e2e-password"

        let graph = IosGraph.shared.instance
        _ = try await graph.awaitReady()
        let api = graph.doNewApi(serverUrl: server)
        let signedIn = try await api.signIn(username: user, password: password, device: graph.device)
        graph.onSignedIn(serverUrl: server, api: api, result: signedIn)
        do {
            try await backUpAndCheck(graph: graph, api: api, images: images)
        } catch {
            try await signOut(graph)
            throw error
        }
        // Awaited: the device is signed out on the server before the test ends.
        try await signOut(graph)
        let web = try await WebSession(server: server, user: user, password: password)
        let live = try await web.liveDeviceIds()
        XCTAssertFalse(live.contains(signedIn.device.id), "still signed in")
    }

    private func signOut(_ graph: AppGraph) async throws {
        graph.signOut()
        _ = try await graph.awaitSignedOut()
    }

    private func backUpAndCheck(graph: AppGraph, api: SquirrelApi, images: Int) async throws {
        try await GraphRunner(graph: { graph }).run(deadline: nil, manual: true, progress: { _, _ in })

        let counts = graph.queue.counts()
        let done = counts.first { $0.key.name == "Done" }?.value.int64Value ?? 0
        XCTAssertEqual(done, Int64(images), "queue: \(counts)")

        // Through the API: the server has every photo's content (uploaded now or already there).
        var hashes: [String: String] = [:]
        PHAsset.fetchAssets(with: .image, options: nil).enumerateObjects { asset, _, _ in
            hashes[asset.localIdentifier] = graph.queue.get(id: asset.localIdentifier)?.sha256
        }
        XCTAssertEqual(hashes.count, images, "every photo hashed")
        let missing = try await api.missingOnServer(hashes: hashes)
        XCTAssertEqual(missing, [])
    }
}

/// A browser-style session on the server (cookie login), to see the devices as the web does.
private struct WebSession {
    let server: String
    let session = URLSession(configuration: .ephemeral)

    init(server: String, user: String, password: String) async throws {
        self.server = server
        var request = URLRequest(url: URL(string: "\(server)/api/auth/login")!)
        request.httpMethod = "POST"
        request.setValue("application/json", forHTTPHeaderField: "Content-Type")
        request.httpBody = try JSONSerialization.data(withJSONObject: ["username": user, "password": password])
        let (_, response) = try await session.data(for: request)
        XCTAssertEqual((response as? HTTPURLResponse)?.statusCode, 200, "web login")
    }

    /// The owner's devices that are still signed in.
    func liveDeviceIds() async throws -> Set<String> {
        struct Device: Decodable { let id: String; let revoked_at: String? }
        let (data, _) = try await session.data(from: URL(string: "\(server)/api/devices")!)
        return Set(try JSONDecoder().decode([Device].self, from: data).filter { $0.revoked_at == nil }.map(\.id))
    }
}
