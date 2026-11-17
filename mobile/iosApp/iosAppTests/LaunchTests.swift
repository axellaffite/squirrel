import XCTest
import SquirrelKit

/// The Kotlin framework is linked and its graph builds inside the host app.
final class LaunchTests: XCTestCase {
    func testTheKotlinGraphStarts() {
        XCTAssertEqual(IosGraph.shared.instance.device.platform.name, "ios")
    }
}
