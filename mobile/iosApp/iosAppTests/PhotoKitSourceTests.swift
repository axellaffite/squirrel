import Photos
import UIKit
import XCTest
import SquirrelKit

/// The Kotlin PhotoKit source against the simulator's library. Needs photo access granted to the
/// host app first: `xcrun simctl privacy booted grant photos app.squirrel.ios`, and a few photos
/// (`xcrun simctl addmedia booted <files>`).
final class PhotoKitSourceTests: XCTestCase {
    private let filter = SourceFilter(includeVideos: false, albums: AlbumFilterAll.shared)

    override func setUp() async throws {
        guard PHPhotoLibrary.authorizationStatus(for: .readWrite) == .authorized else {
            throw XCTSkip("Grant photo access to app.squirrel.ios first")
        }
    }

    func testAFullScanThenOnlyWhatChanged() async throws {
        let source = PhotoKitSource(exporter: AssetExporter(dir: NSTemporaryDirectory() + "export-tests"), allowNetwork: { true })
        let full = try await source.changes(token: nil, filter: filter)
        XCTAssertTrue(full.full)
        let images = PHAsset.fetchAssets(with: .image, options: nil).count
        XCTAssertEqual(full.assets.count, images)
        XCTAssertGreaterThan(images, 0, "add photos with simctl addmedia first")

        let added = try await addPhoto()
        let changed = try await source.changes(token: full.token, filter: filter)
        XCTAssertFalse(changed.full)
        XCTAssertEqual(changed.assets.map { $0.deviceAssetId }, [added])
        let asset = try XCTUnwrap(changed.assets.first)
        XCTAssertEqual(asset.kind, "photo")
        XCTAssertEqual(asset.size, -1) // known after export
        XCTAssertNotNil(asset.takenAt)
    }

    func testTheExportIsTheOriginalBytes() async throws {
        let source = PhotoKitSource(exporter: AssetExporter(dir: NSTemporaryDirectory() + "export-tests"), allowNetwork: { true })
        let full = try await source.changes(token: nil, filter: filter)
        let first = try XCTUnwrap(full.assets.first)

        let reader = try await XCTUnwrapAsync(try await source.open(deviceAssetId: first.deviceAssetId))
        let exported = try await readAll(reader)
        reader.close()
        XCTAssertEqual(Int64(exported.count), reader.size)

        let asset = try XCTUnwrap(PHAsset.fetchAssets(withLocalIdentifiers: [first.deviceAssetId], options: nil).firstObject)
        let resource = try XCTUnwrap(PHAssetResource.assetResources(for: asset).first { $0.type == .photo || $0.type == .fullSizePhoto })
        let original = try await requestData(resource)
        XCTAssertEqual(exported, original)
    }

    private func readAll(_ reader: AssetReader) async throws -> Data {
        var out = Data()
        let buffer = KotlinByteArray(size: 64 * 1024)
        var position: Int64 = 0
        while true {
            let count = try await reader.read(position: position, into: buffer, offset: 0, length: buffer.size).int32Value
            if count < 0 { break }
            for i in 0..<count { out.append(UInt8(bitPattern: buffer.get(index: i))) }
            position += Int64(count)
        }
        return out
    }

    private func requestData(_ resource: PHAssetResource) async throws -> Data {
        try await withCheckedThrowingContinuation { continuation in
            let collected = Collected()
            PHAssetResourceManager.default().requestData(for: resource, options: nil) { chunk in
                collected.data.append(chunk)
            } completionHandler: { error in
                if let error { continuation.resume(throwing: error) } else { continuation.resume(returning: collected.data) }
            }
        }
    }

    /// Adds a small generated photo; returns its local identifier.
    private func addPhoto() async throws -> String {
        let image = UIGraphicsImageRenderer(size: CGSize(width: 64, height: 48)).image { context in
            UIColor.systemTeal.setFill()
            context.fill(CGRect(x: 0, y: 0, width: 64, height: 48))
        }
        try await PHPhotoLibrary.shared().performChanges {
            PHAssetChangeRequest.creationRequestForAsset(from: image)
        }
        let newest = PHFetchOptions()
        newest.sortDescriptors = [NSSortDescriptor(key: "creationDate", ascending: false)]
        newest.fetchLimit = 1
        return try XCTUnwrap(PHAsset.fetchAssets(with: .image, options: newest).firstObject).localIdentifier
    }
}

private final class Collected: @unchecked Sendable {
    var data = Data()
}

func XCTUnwrapAsync<T>(_ value: T?, file: StaticString = #filePath, line: UInt = #line) async throws -> T {
    try XCTUnwrap(value, file: file, line: line)
}
