import SwiftUI
import SquirrelKit

@main
struct SquirrelApp: App {
    @UIApplicationDelegateAdaptor(AppDelegate.self) var appDelegate
    var body: some Scene {
        WindowGroup { ComposeView().ignoresSafeArea() } // Compose handles insets itself
    }
}

struct ComposeView: UIViewControllerRepresentable {
    func makeUIViewController(context: Context) -> UIViewController { MainViewControllerKt.MainViewController() }
    func updateUIViewController(_ controller: UIViewController, context: Context) {}
}
