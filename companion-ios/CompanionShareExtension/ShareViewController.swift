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

        var candidates: [SharedImportEnvelope] = []
        for provider in providers {
            if let envelope = await envelope(from: provider, extraNote: nil) {
                candidates.append(envelope)
            }
        }

        if var selected = preferredEnvelope(from: candidates) {
            selected.debugTrail = buildSelectionDebugTrail(from: candidates, selectedID: selected.id)
            sharedStore.enqueue(selected)
            finish(with: "Saved. Open Companion to continue.")
            return
        }

        finish(with: "Could not read this shared item yet.")
    }

    private func finish(with message: String) {
        DispatchQueue.main.async {
            self.activityIndicator.stopAnimating()
            self.statusLabel.text = message
            self.extensionContext?.completeRequest(returningItems: nil)
        }
    }

    private func envelope(from provider: NSItemProvider, extraNote: String?) async -> SharedImportEnvelope? {
        if provider.hasItemConformingToTypeIdentifier(UTType.fileURL.identifier),
           let sharedFile = await loadSharedFile(from: provider) {
            return envelope(for: sharedFile, extraNote: extraNote)
        }

        if provider.hasItemConformingToTypeIdentifier(UTType.url.identifier),
           let url = await loadURL(from: provider) {
            if url.isFileURL,
               let sharedFile = copySharedFile(from: url, uniformTypeIdentifier: UTType.fileURL.identifier) {
                return envelope(for: sharedFile, extraNote: extraNote)
            }
            let classification = classification(forURL: url)
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
                classification: classification,
                disposition: classification == .unsupportedUnknown ? .diagnosticsOnly : .directHomePreview,
                note: extraNote,
                debugTrail: nil
            )
        }

        if provider.hasItemConformingToTypeIdentifier(UTType.plainText.identifier),
           let text = await loadText(from: provider) {
            let trimmed = text.trimmingCharacters(in: .whitespacesAndNewlines)
            guard !trimmed.isEmpty else {
                return nil
            }
            return SharedImportEnvelope(
                id: UUID().uuidString,
                sourceApplication: nil,
                receivedAt: Date(),
                rawKind: .plainText,
                mimeType: "text/plain",
                uniformTypeIdentifier: UTType.plainText.identifier,
                fileName: nil,
                fileSizeBytes: trimmed.lengthOfBytes(using: .utf8),
                originalText: trimmed,
                originalURL: extractURL(from: trimmed),
                storedFilePath: nil,
                classification: classification(forText: trimmed),
                disposition: disposition(forText: trimmed),
                note: extraNote,
                debugTrail: nil
            )
        }

        if let sharedFile = await loadSharedFile(from: provider) {
            return envelope(for: sharedFile, extraNote: extraNote)
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
            note: extraNote ?? "Unsupported shared item type.",
            debugTrail: nil
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

    private func envelope(for sharedFile: LoadedSharedFile, extraNote: String?) -> SharedImportEnvelope {
        let classification = classification(forFileName: sharedFile.fileName)
        let disposition: SharedImportDisposition
        switch classification {
        case .gpxFile, .fitFile, .tcxFile:
            disposition = .directHomePreview
        case .genericXMLFile, .googleMapsLocationLink, .genericLocationLink, .plainCoordinates, .unsupportedUnknown:
            disposition = .diagnosticsOnly
        }
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
            disposition: disposition,
            note: extraNote,
            debugTrail: nil
        )
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

    private func copySharedFile(from url: URL, uniformTypeIdentifier: String?) -> LoadedSharedFile? {
        guard let storedPath = try? sharedStore.copyFileIntoSharedContainer(
            sourceURL: url,
            suggestedName: url.lastPathComponent
        ) else {
            return nil
        }
        return LoadedSharedFile(
            storedFilePath: storedPath,
            fileName: url.lastPathComponent,
            fileSizeBytes: try? url.resourceValues(forKeys: [.fileSizeKey]).fileSize,
            originalURL: url.absoluteString,
            uniformTypeIdentifier: uniformTypeIdentifier
        )
    }

    private func extractURL(from text: String) -> String? {
        text
            .components(separatedBy: .newlines)
            .map { $0.trimmingCharacters(in: .whitespacesAndNewlines) }
            .first(where: { $0.starts(with: "http://") || $0.starts(with: "https://") })
    }

    private func preferredEnvelope(from envelopes: [SharedImportEnvelope]) -> SharedImportEnvelope? {
        envelopes.max { lhs, rhs in
            sortKey(for: lhs) < sortKey(for: rhs)
        }
    }

    private func sortKey(for envelope: SharedImportEnvelope) -> Int {
        switch (envelope.classification, envelope.rawKind) {
        case (.gpxFile, .file):
            return 100
        case (.googleMapsLocationLink, .url):
            return 95
        case (.genericLocationLink, .url):
            return 90
        case (.fitFile, .file), (.tcxFile, .file):
            return 80
        case (.plainCoordinates, .plainText):
            return 70
        case (.googleMapsLocationLink, .plainText):
            return 60
        case (.genericLocationLink, .plainText):
            return 55
        case (_, _) where envelope.disposition == .directHomePreview:
            return 40
        case (.unsupportedUnknown, _):
            return 0
        default:
            return 10
        }
    }

    private func buildSelectionDebugTrail(from envelopes: [SharedImportEnvelope], selectedID: String) -> [String] {
        var lines: [String] = [
            "share-extension candidates=\(envelopes.count)",
            "selected-id=\(selectedID)"
        ]
        let ranked = envelopes.sorted { lhs, rhs in
            let lhsKey = sortKey(for: lhs)
            let rhsKey = sortKey(for: rhs)
            if lhsKey == rhsKey {
                return lhs.id < rhs.id
            }
            return lhsKey > rhsKey
        }
        for (index, envelope) in ranked.enumerated() {
            let payload = envelope.originalURL ?? envelope.originalText ?? envelope.fileName ?? "none"
            lines.append(
                "candidate[\(index)] score=\(sortKey(for: envelope)) id=\(envelope.id) raw=\(envelope.rawKind.rawValue) class=\(envelope.classification.rawValue) disp=\(envelope.disposition.rawValue) payload=\(String(payload.prefix(160)))"
            )
        }
        return lines
    }

    private func classification(forText text: String) -> SharedImportClassification {
        if let url = extractURL(from: text), let parsed = URL(string: url) {
            return classification(forURL: parsed)
        }
        if extractCoordinate(from: text) != nil {
            return text.contains("http://") || text.contains("https://") ? .genericLocationLink : .plainCoordinates
        }
        return .unsupportedUnknown
    }

    private func disposition(forText text: String) -> SharedImportDisposition {
        classification(forText: text) == .unsupportedUnknown ? .diagnosticsOnly : .directHomePreview
    }

    private func classification(forURL url: URL) -> SharedImportClassification {
        if isGoogleMapsHost(url.host) {
            return .googleMapsLocationLink
        }
        if isKnownMapHost(url.host) || extractCoordinate(from: url.absoluteString) != nil {
            return .genericLocationLink
        }
        return .unsupportedUnknown
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

    private func isKnownMapHost(_ host: String?) -> Bool {
        guard let host = host?.lowercased() else { return false }
        return host.contains("openstreetmap.org")
            || host.contains("garmin.com")
            || host.contains("strava.com")
            || host.contains("komoot.")
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
