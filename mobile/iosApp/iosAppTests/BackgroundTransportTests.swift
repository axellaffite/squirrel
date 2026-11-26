import XCTest
import SquirrelKit

/// Answers every request with `status` and remembers what it saw (a foreground session in tests:
/// background sessions do not take custom protocols; the Kotlin delegate is the same).
final class StubProtocol: URLProtocol {
    nonisolated(unsafe) static var status = 204
    nonisolated(unsafe) static var seen: [URLRequest] = []

    override class func canInit(with request: URLRequest) -> Bool { true }
    override class func canonicalRequest(for request: URLRequest) -> URLRequest { request }

    override func startLoading() {
        StubProtocol.seen.append(request)
        let response = HTTPURLResponse(url: request.url!, statusCode: StubProtocol.status, httpVersion: nil, headerFields: nil)!
        client?.urlProtocol(self, didReceive: response, cacheStoragePolicy: .notAllowed)
        client?.urlProtocolDidFinishLoading(self)
    }

    override func stopLoading() {}
}

final class BackgroundTransportTests: XCTestCase {
    private let chunkDir = NSTemporaryDirectory() + "chunk-tests"
    private var store: TransportStore!
    private var transfers: TransferSession!

    override func setUp() {
        StubProtocol.status = 204
        StubProtocol.seen = []
        let database = DatabaseKt.createDatabase(driver: DriverKt.createIosDriver(name: "transport-tests.db", directory: nil))
        store = TransportStore(db: database, now: { KotlinLong(value: Int64(Date().timeIntervalSince1970 * 1000)) })
        let configuration = URLSessionConfiguration.ephemeral
        configuration.protocolClasses = [StubProtocol.self]
        transfers = TransferSession(configuration: configuration, store: store, chunkDir: chunkDir, onEventsFinished: {})
    }

    private func session(token: String?) -> UploadSession {
        UploadSession(uploadId: "up-1", chunkSize: 4, chunkCount: 3, received: [], uploadToken: token)
    }

    private func reader() -> ByteArrayReader {
        let bytes = KotlinByteArray(size: 10) // chunks of 4, 4 and 2 bytes
        for i in 0..<10 { bytes.set(index: Int32(i), value: Int8(i)) }
        return ByteArrayReader(bytes: bytes, modifiedAt: nil)
    }

    func testAChunkGoesWithTheUploadTokenAndItsFileIsRemoved() async throws {
        let transport = BackgroundTransport(baseUrl: "http://localhost:8188", transfers: transfers, wifiOnly: { KotlinBoolean(value: false) })

        try await transport.put(session: session(token: "squ_test"), index: 2, reader: reader())

        let request = try XCTUnwrap(StubProtocol.seen.last)
        XCTAssertEqual(request.httpMethod, "PUT")
        XCTAssertEqual(request.url?.absoluteString, "http://localhost:8188/api/uploads/up-1/chunks/2")
        XCTAssertEqual(request.value(forHTTPHeaderField: "Authorization"), "Upload squ_test")
        XCTAssertEqual(request.value(forHTTPHeaderField: "Content-Length"), "2") // the last, short chunk
        XCTAssertFalse(FileManager.default.fileExists(atPath: transfers.chunkPath(task: "up-1:2")))
        XCTAssertNil(store.take(task: "up-1:2")) // consumed by the transport: never seen twice
    }

    func testARefusedTokenAsksForAFreshSession() async throws {
        StubProtocol.status = 401
        let transport = BackgroundTransport(baseUrl: "http://localhost:8188", transfers: transfers, wifiOnly: { KotlinBoolean(value: false) })
        do {
            try await transport.put(session: session(token: "squ_old"), index: 0, reader: reader())
            XCTFail("a 401 must not pass")
        } catch {
            XCTAssertTrue((error as NSError).kotlinException is UploadTokenRefused, "\(error)")
        }
    }

    func testAResultThatArrivedWhileNobodyWaitedIsUsedOnce() async throws {
        store.record(task: "up-1:1", result: ChunkResult(status: 204, error: nil, cancelled: false))
        let transport = BackgroundTransport(baseUrl: "http://localhost:8188", transfers: transfers, wifiOnly: { KotlinBoolean(value: false) })

        try await transport.put(session: session(token: "squ_test"), index: 1, reader: reader())

        XCTAssertTrue(StubProtocol.seen.isEmpty) // finished in the background: not sent again
        XCTAssertNil(store.take(task: "up-1:1"))
    }
}
