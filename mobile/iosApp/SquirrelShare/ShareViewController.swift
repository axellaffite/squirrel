import UIKit
import UniformTypeIdentifiers
import SquirrelKit

/// "Squirrel" in the share sheet: copies the shared files out of the share, then hands them to
/// the Kotlin send screen (`ShareViewController.kt`), which sends them and completes the request.
final class ShareViewController: UIViewController {
    override func viewDidLoad() {
        super.viewDidLoad()
        let providers = (extensionContext?.inputItems as? [NSExtensionItem] ?? []).flatMap { $0.attachments ?? [] }
        Task { @MainActor in
            var paths: [String] = []
            for provider in providers {
                if let path = await Self.copy(provider) { paths.append(path) }
            }
            show(ShareViewControllerKt.ShareViewController(paths: paths) { [weak self] in
                self?.extensionContext?.completeRequest(returningItems: nil)
            })
        }
    }

    private func show(_ child: UIViewController) {
        addChild(child)
        child.view.frame = view.bounds
        child.view.autoresizingMask = [.flexibleWidth, .flexibleHeight]
        view.addSubview(child.view)
        child.didMove(toParent: self)
    }

    /// The attachment as a file of our own (the system deletes its copy once the handler returns),
    /// named as the sharing app named it; nil when it holds no file (a bare link or text).
    private static func copy(_ provider: NSItemProvider) async -> String? {
        let type = provider.registeredTypeIdentifiers.first { UTType($0)?.conforms(to: .data) == true && UTType($0)?.conforms(to: .url) != true }
        guard let type else { return nil }
        let suggested = provider.suggestedName
        return await withCheckedContinuation { continuation in
            _ = provider.loadFileRepresentation(forTypeIdentifier: type) { url, _ in
                guard let url else { return continuation.resume(returning: nil) }
                let manager = FileManager.default
                let folder = manager.temporaryDirectory.appendingPathComponent(UUID().uuidString, isDirectory: true)
                var name = suggested ?? url.lastPathComponent
                if (name as NSString).pathExtension.isEmpty, !url.pathExtension.isEmpty { name += "." + url.pathExtension }
                let target = folder.appendingPathComponent(name)
                do {
                    try manager.createDirectory(at: folder, withIntermediateDirectories: true)
                    try manager.copyItem(at: url, to: target)
                    continuation.resume(returning: target.path)
                } catch {
                    continuation.resume(returning: nil)
                }
            }
        }
    }
}
