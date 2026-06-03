import UIKit
import UniformTypeIdentifiers

final class ShareViewController: UIViewController {
    private static let companionURL = URL(string: "navon://import")!

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
        onSuccess: { [weak self] _ in self?.finishAfterSaving() },
        onError: { [weak self] msg in self?.finish(with: msg) }
    )

    override func viewDidLoad() {
        super.viewDidLoad()
        view.backgroundColor = .systemBackground
        statusLabel.text = "Importing into Companion..."
        statusLabel.textAlignment = .center
        statusLabel.numberOfLines = 0
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
            self.extensionContext?.completeRequest(returningItems: nil)
        }
    }

    private func finishAfterSaving() {
        DispatchQueue.main.async {
            self.activityIndicator.stopAnimating()
            self.statusLabel.text = "Saved. Opening Companion..."
            self.extensionContext?.open(Self.companionURL) { success in
                DispatchQueue.main.async {
                    self.statusLabel.text = success ? "Opening Companion..." : "Saved. Open Companion to continue."
                    self.extensionContext?.completeRequest(returningItems: nil)
                }
            }
        }
    }
}
