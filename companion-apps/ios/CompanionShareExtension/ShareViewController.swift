import UIKit
import UniformTypeIdentifiers

// Share extension (com.apple.share-services). On modern iOS, Apple does not
// allow share extensions to programmatically foreground the host app — neither
// NSExtensionContext.open nor the responder-chain openURL: hack works. Switching
// to an action extension (com.apple.ui-services) does enable foregrounding, but
// many sharing apps (Google Maps, Safari for URLs, …) don't expose their share
// items to action extensions, so the host app stops appearing in their share
// sheets entirely. The top-row placement is more valuable than auto-foreground,
// so we stay on share-services and let the user tap Navon to finish the import.
// The enqueued item is drained on next foreground via
// ShareImportService.consumePendingSharedImports.
final class ShareViewController: UIViewController {
    private let statusLabel = UILabel()
    private let activityIndicator = UIActivityIndicatorView(style: .large)
    private let sharedStore = SharedImportStore()
    private var didProcess = false

    private lazy var viewModel = ShareImportViewModel(
        sharedStore: sharedStore,
        makeDebugContext: { [unowned self] in
            let bundle = Bundle.main
            return SharedImportDebugContext(
                producerTarget: "share-extension",
                producerBundleID: bundle.bundleIdentifier,
                producerVersion: bundle.infoDictionary?["CFBundleShortVersionString"] as? String,
                producerBuild: bundle.infoDictionary?["CFBundleVersion"] as? String,
                latestHandlerTarget: "share-extension",
                latestHandlerBundleID: bundle.bundleIdentifier,
                latestHandlerVersion: bundle.infoDictionary?["CFBundleShortVersionString"] as? String,
                latestHandlerBuild: bundle.infoDictionary?["CFBundleVersion"] as? String,
                latestPhase: "share-extension.enqueue",
                latestOutcome: "selected",
                lastUpdatedAt: Date()
            )
        },
        onSuccess: { [weak self] _ in self?.finish(with: "Saved to Navon. Open the app to view.") },
        onError: { [weak self] message in self?.finish(with: message) }
    )

    override func viewDidLoad() {
        super.viewDidLoad()
        view.backgroundColor = .systemBackground
        statusLabel.text = "Saving to Navon…"
        statusLabel.textAlignment = .center
        statusLabel.numberOfLines = 0
        statusLabel.translatesAutoresizingMaskIntoConstraints = false
        activityIndicator.translatesAutoresizingMaskIntoConstraints = false
        activityIndicator.startAnimating()

        let stack = UIStackView(arrangedSubviews: [activityIndicator, statusLabel])
        stack.axis = .vertical
        stack.spacing = 16
        stack.alignment = .center
        stack.translatesAutoresizingMaskIntoConstraints = false
        view.addSubview(stack)
        NSLayoutConstraint.activate([
            stack.centerXAnchor.constraint(equalTo: view.centerXAnchor),
            stack.centerYAnchor.constraint(equalTo: view.centerYAnchor),
            stack.leadingAnchor.constraint(greaterThanOrEqualTo: view.leadingAnchor, constant: 24),
            stack.trailingAnchor.constraint(lessThanOrEqualTo: view.trailingAnchor, constant: -24),
        ])
    }

    override func viewDidAppear(_ animated: Bool) {
        super.viewDidAppear(animated)
        guard !didProcess else { return }
        didProcess = true
        Task {
            await viewModel.process(items: (extensionContext?.inputItems as? [NSExtensionItem]) ?? [])
        }
    }

    private func finish(with message: String) {
        DispatchQueue.main.async {
            self.activityIndicator.stopAnimating()
            self.statusLabel.text = message
            // Brief confirmation so the user knows the item was captured, then
            // auto-dismiss. They'll see the imported item next time they open Navon.
            DispatchQueue.main.asyncAfter(deadline: .now() + 0.8) {
                self.extensionContext?.completeRequest(returningItems: nil)
            }
        }
    }
}
