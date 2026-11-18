import XCTest
import SquirrelKit

/// The Kotlin framework is linked and its graph builds inside the host app.
final class LaunchTests: XCTestCase {
    func testTheKotlinGraphStarts() {
        XCTAssertEqual(IosGraph.shared.instance.device.platform.name, "ios")
    }

    /// Every permission the app may ask for explains itself in the app's words.
    func testThePermissionPromptsAreExplained() {
        for key in ["NSPhotoLibraryUsageDescription", "NSLocalNetworkUsageDescription"] {
            let text = Bundle.main.object(forInfoDictionaryKey: key) as? String
            XCTAssertFalse(text?.isEmpty ?? true, key)
        }
    }
}
