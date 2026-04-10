import UIKit
import UniformTypeIdentifiers

final class ShareViewController: UIViewController {
    private struct LoadedSharedFile {
        let storedFilePath: String
        let fileName: String
        let fileSizeBytes: Int?
        let originalURL: String?
        let uniformTypeIdentifier: String?
    }

    private let statusLabel = UILabel()
    private let activityIndicator = UIActivityIndicatorView(style: .large)
    private let sharedStore = SharedImportStore()
    private var didProcess = false

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
            await processInputItems()
        }
    }

    private func processInputItems() async {
        let items = (extensionContext?.inputItems as? [NSExtensionItem]) ?? []
        let providers = items.flatMap { $0.attachments ?? [] }
        guard !providers.isEmpty else {
            finish(with: "Nothing to import.")
            return
        }

        var savedAny = false
        for (index, provider) in providers.enumerated() {
            if let envelope = await envelope(from: provider, extraNote: index > 0 ? "Additional shared item ignored in v1." : nil) {
                sharedStore.enqueue(envelope)
                savedAny = true
            }
        }

        finish(with: savedAny ? "Saved. Open Companion to continue." : "Could not read this shared item yet.")
    }

    private func finish(with message: String) {
        DispatchQueue.main.async {
            self.activityIndicator.stopAnimating()
            self.statusLabel.text = message
            self.extensionContext?.completeRequest(returningItems: nil)
        }
    }

    private func envelope(from provider: NSItemProvider, extraNote: String?) async -> SharedImportEnvelope? {
        if provider.hasItemConformingToTypeIdentifier(UTType.url.identifier),
           let url = await loadURL(from: provider) {
            return SharedImportEnvelope(
                id: UUID().uuidString,
                sourceApplication: nil,
                receivedAt: Date(),
                rawKind: .url,
                mimeType: nil,
                uniformTypeIdentifier: UTType.url.identifier,
                fileName: nil,
                fileSizeBytes: nil,
                originalText: nil,
                originalURL: url.absoluteString,
                storedFilePath: nil,
                classification: isGoogleMapsHost(url.host) ? .googleMapsLocationLink : .genericLocationLink,
                disposition: .directHomePreview,
                note: extraNote
            )
        }

        if provider.hasItemConformingToTypeIdentifier(UTType.plainText.identifier),
           let text = await loadText(from: provider) {
            return SharedImportEnvelope(
                id: UUID().uuidString,
                sourceApplication: nil,
                receivedAt: Date(),
                rawKind: .plainText,
                mimeType: "text/plain",
                uniformTypeIdentifier: UTType.plainText.identifier,
                fileName: nil,
                fileSizeBytes: text.lengthOfBytes(using: .utf8),
                originalText: text,
                originalURL: extractURL(from: text),
                storedFilePath: nil,
                classification: classification(forText: text),
                disposition: disposition(forText: text),
                note: extraNote
            )
        }

        if let sharedFile = await loadSharedFile(from: provider) {
            let classification = classification(forFileName: sharedFile.fileName)
            return SharedImportEnvelope(
                id: UUID().uuidString,
                sourceApplication: nil,
                receivedAt: Date(),
                rawKind: .file,
                mimeType: nil,
                uniformTypeIdentifier: sharedFile.uniformTypeIdentifier,
                fileName: sharedFile.fileName,
                fileSizeBytes: sharedFile.fileSizeBytes,
                originalText: nil,
                originalURL: sharedFile.originalURL,
                storedFilePath: sharedFile.storedFilePath,
                classification: classification,
                disposition: classification == .gpxFile ? .directHomePreview : .diagnosticsOnly,
                note: extraNote
            )
        }

        return SharedImportEnvelope(
            id: UUID().uuidString,
            sourceApplication: nil,
            receivedAt: Date(),
            rawKind: .unknown,
            mimeType: nil,
            uniformTypeIdentifier: provider.registeredTypeIdentifiers.first,
            fileName: nil,
            fileSizeBytes: nil,
            originalText: nil,
            originalURL: nil,
            storedFilePath: nil,
            classification: .unsupportedUnknown,
            disposition: .diagnosticsOnly,
            note: extraNote ?? "Unsupported shared item type."
        )
    }

    private func loadURL(from provider: NSItemProvider) async -> URL? {
        await withCheckedContinuation { continuation in
            provider.loadItem(forTypeIdentifier: UTType.url.identifier, options: nil) { item, _ in
                continuation.resume(returning: item as? URL)
            }
        }
    }

    private func loadText(from provider: NSItemProvider) async -> String? {
        await withCheckedContinuation { continuation in
            provider.loadItem(forTypeIdentifier: UTType.plainText.identifier, options: nil) { item, _ in
                if let text = item as? String {
                    continuation.resume(returning: text)
                } else if let data = item as? Data {
                    continuation.resume(returning: String(data: data, encoding: .utf8))
                } else {
                    continuation.resume(returning: nil)
                }
            }
        }
    }

    private func loadSharedFile(from provider: NSItemProvider) async -> LoadedSharedFile? {
        if provider.hasItemConformingToTypeIdentifier(UTType.fileURL.identifier) {
            return await withCheckedContinuation { continuation in
                provider.loadItem(forTypeIdentifier: UTType.fileURL.identifier, options: nil) { item, _ in
                    guard let url = item as? URL,
                          let storedPath = try? self.sharedStore.copyFileIntoSharedContainer(
                              sourceURL: url,
                              suggestedName: url.lastPathComponent
                          ) else {
                        continuation.resume(returning: nil)
                        return
                    }
                    continuation.resume(
                        returning: LoadedSharedFile(
                            storedFilePath: storedPath,
                            fileName: url.lastPathComponent,
                            fileSizeBytes: try? url.resourceValues(forKeys: [.fileSizeKey]).fileSize,
                            originalURL: url.absoluteString,
                            uniformTypeIdentifier: UTType.fileURL.identifier
                        )
                    )
                }
            }
        }
        return await withCheckedContinuation { continuation in
            provider.loadFileRepresentation(forTypeIdentifier: provider.registeredTypeIdentifiers.first ?? UTType.item.identifier) { url, _ in
                guard let url,
                      let storedPath = try? self.sharedStore.copyFileIntoSharedContainer(
                          sourceURL: url,
                          suggestedName: url.lastPathComponent
                      ) else {
                    continuation.resume(returning: nil)
                    return
                }
                continuation.resume(
                    returning: LoadedSharedFile(
                        storedFilePath: storedPath,
                        fileName: url.lastPathComponent,
                        fileSizeBytes: try? url.resourceValues(forKeys: [.fileSizeKey]).fileSize,
                        originalURL: url.absoluteString,
                        uniformTypeIdentifier: provider.registeredTypeIdentifiers.first
                    )
                )
            }
        }
    }

    private func extractURL(from text: String) -> String? {
        text
            .components(separatedBy: .newlines)
            .map { $0.trimmingCharacters(in: .whitespacesAndNewlines) }
            .first(where: { $0.starts(with: "http://") || $0.starts(with: "https://") })
    }

    private func classification(forText text: String) -> SharedImportClassification {
        if let url = extractURL(from: text), let parsed = URL(string: url), isGoogleMapsHost(parsed.host) {
            return .googleMapsLocationLink
        }
        if extractCoordinate(from: text) != nil {
            return text.contains("http://") || text.contains("https://") ? .genericLocationLink : .plainCoordinates
        }
        return .unsupportedUnknown
    }

    private func disposition(forText text: String) -> SharedImportDisposition {
        classification(forText: text) == .unsupportedUnknown ? .diagnosticsOnly : .directHomePreview
    }

    private func classification(forFileName fileName: String) -> SharedImportClassification {
        let lower = fileName.lowercased()
        if lower.hasSuffix(".gpx") { return .gpxFile }
        if lower.hasSuffix(".fit") { return .fitFile }
        if lower.hasSuffix(".tcx") { return .tcxFile }
        if lower.hasSuffix(".xml") { return .genericXMLFile }
        return .unsupportedUnknown
    }

    private func isGoogleMapsHost(_ host: String?) -> Bool {
        guard let host = host?.lowercased() else { return false }
        return host.contains("google.") || host == "maps.app.goo.gl" || host == "goo.gl"
    }

    private func extractCoordinate(from value: String) -> (Double, Double)? {
        let pattern = try? NSRegularExpression(pattern: "(-?\\d{1,3}\\.\\d+)[,\\s]+(-?\\d{1,3}\\.\\d+)")
        guard let match = pattern?.firstMatch(in: value, range: NSRange(location: 0, length: value.utf16.count)),
              let latitudeRange = Range(match.range(at: 1), in: value),
              let longitudeRange = Range(match.range(at: 2), in: value),
              let latitude = Double(value[latitudeRange]),
              let longitude = Double(value[longitudeRange]),
              (-90.0...90.0).contains(latitude),
              (-180.0...180.0).contains(longitude) else {
            return nil
        }
        return (latitude, longitude)
    }
}
