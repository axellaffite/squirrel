import UIKit
import SquirrelKit

/// Glue only: these two callbacks reach the app delegate and nothing else.
final class AppDelegate: NSObject, UIApplicationDelegate {
    func application(_ application: UIApplication,
                     didFinishLaunchingWithOptions options: [UIApplication.LaunchOptionsKey: Any]? = nil) -> Bool {
        IosEntryKt.onLaunch() // registers BGTask handlers: must happen before launch finishes
        return true
    }

    func application(_ application: UIApplication,
                     handleEventsForBackgroundURLSession identifier: String,
                     completionHandler: @escaping () -> Void) {
        IosEntryKt.onBackgroundUrlSessionEvents(identifier: identifier, completionHandler: completionHandler)
    }
}
