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
            try await backUpAndCheck(graph: graph, server: server, user: user, password: password,
                                     deviceName: signedIn.device.name, images: images)
        } catch {
            try await signOut(graph)
            throw error
        }
        // Awaited: the device is signed out on the server before the test ends.
        try await signOut(graph)
        let web = try await WebSession(server: server, user: user, password: password)
        XCTAssertFalse(try await web.liveDeviceIds().contains(signedIn.device.id), "still signed in")
    }

    private func signOut(_ graph: AppGraph) async throws {
        graph.signOut()
        _ = try await graph.awaitSignedOut()
    }

    private func backUpAndCheck(graph: AppGraph, server: String, user: String, password: String,
                                deviceName: String, images: Int) async throws {
        try await GraphRunner(graph: { graph }).run(deadline: nil, manual: true, progress: { _, _ in })

        let counts = graph.queue.counts()
        let done = counts.first { $0.key.name == "Done" }?.value.int64Value ?? 0
        XCTAssertEqual(done, Int64(images), "queue: \(counts)")

        // Through the API: every photo sits in Photos/<device>/<year>/.
        let web = try await WebSession(server: server, user: user, password: password)
        let photos = try await web.child(named: "Photos", of: nil)
        let device = try await web.child(named: deviceName, of: photos)
        let years = try await web.list(device).filter { $0.kind == "folder" }
        XCTAssertFalse(years.isEmpty)
        var files = 0
        for year in years {
            XCTAssertNotNil(Int(year.name), "year folder \(year.name)")
            files += try await web.list(year.id).filter { $0.kind == "file" }.count
        }
        // A photo whose bytes the server already has is usually linked rather than stored again
        // (two such photos backed up in the same run can both land), so the count lies between
        // the distinct contents and the photos.
        XCTAssertGreaterThanOrEqual(files, distinctContents(graph.queue))
        XCTAssertLessThanOrEqual(files, images)
    }
}

/// How many distinct contents the library's photos have, from the hashes the queue recorded.
private func distinctContents(_ queue: Queue) -> Int {
    var hashes = Set<String>()
    PHAsset.fetchAssets(with: .image, options: nil).enumerateObjects { asset, _, _ in
        hashes.insert(queue.get(id: asset.localIdentifier)?.sha256 ?? asset.localIdentifier)
    }
    return hashes.count
}

/// A browser-style session on the server (cookie login), for reading folders from the test.
private struct WebSession {
    struct Node: Decodable { let id: String; let name: String; let kind: String }
    private struct Listing: Decodable { let items: [Node]; let next_cursor: String? }

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

    func list(_ parent: String?) async throws -> [Node] {
        var nodes: [Node] = []
        var cursor: String?
        repeat {
            var url = URLComponents(string: "\(server)/api/nodes")!
            url.queryItems = [URLQueryItem(name: "limit", value: "500")]
            if let parent { url.queryItems?.append(URLQueryItem(name: "parent_id", value: parent)) }
            if let cursor { url.queryItems?.append(URLQueryItem(name: "cursor", value: cursor)) }
            let (data, _) = try await session.data(from: url.url!)
            let page = try JSONDecoder().decode(Listing.self, from: data)
            nodes += page.items
            cursor = page.next_cursor
        } while cursor != nil
        return nodes
    }

    /// The owner's devices that are still signed in.
    func liveDeviceIds() async throws -> Set<String> {
        struct Device: Decodable { let id: String; let revoked_at: String? }
        struct Devices: Decodable { let items: [Device] }
        let (data, _) = try await session.data(from: URL(string: "\(server)/api/devices")!)
        return Set(try JSONDecoder().decode(Devices.self, from: data).items.filter { $0.revoked_at == nil }.map(\.id))
    }

    func child(named name: String, of parent: String?) async throws -> String {
        let nodes = try await list(parent)
        return try XCTUnwrap(nodes.first { $0.name == name }?.id, "\(name) in \(nodes.map(\.name))")
    }
}
