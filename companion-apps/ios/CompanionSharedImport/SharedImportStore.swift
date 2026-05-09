import Foundation

enum SharedImportStoreConfig {
    static let appGroupIdentifier = "group.me.fiksu.esp32map.companion"
    static let queueKey = "share-import.queue"
    static let queueFileName = "share-import-queue.json"

    static var defaults: UserDefaults? {
        UserDefaults(suiteName: appGroupIdentifier)
    }

    static func containerURL() -> URL? {
        FileManager.default.containerURL(forSecurityApplicationGroupIdentifier: appGroupIdentifier)
    }
}

final class SharedImportStore {
    private let defaults: UserDefaults?
    private let encoder = JSONEncoder()
    private let decoder = JSONDecoder()

    init(defaults: UserDefaults? = SharedImportStoreConfig.defaults) {
        self.defaults = defaults
    }

    func enqueue(_ envelope: SharedImportEnvelope) {
        var queue = loadQueue()
        queue.append(envelope)
        save(queue)
    }

    func drainQueue() -> [SharedImportEnvelope] {
        let queue = loadQueue()
        save([])
        return queue
    }

    func copyFileIntoSharedContainer(sourceURL: URL, suggestedName: String? = nil) throws -> String {
        guard let containerURL = SharedImportStoreConfig.containerURL() else {
            throw CocoaError(.fileNoSuchFile)
        }
        let importsDirectory = containerURL.appendingPathComponent("SharedImports", isDirectory: true)
        try FileManager.default.createDirectory(at: importsDirectory, withIntermediateDirectories: true, attributes: nil)
        let ext = sourceURL.pathExtension.isEmpty ? "dat" : sourceURL.pathExtension
        let fileName = suggestedName ?? "import-\(UUID().uuidString).\(ext)"
        let destinationURL = importsDirectory.appendingPathComponent(fileName)
        if FileManager.default.fileExists(atPath: destinationURL.path) {
            try FileManager.default.removeItem(at: destinationURL)
        }
        try FileManager.default.copyItem(at: sourceURL, to: destinationURL)
        return destinationURL.path
    }

    private func loadQueue() -> [SharedImportEnvelope] {
        if let fileURL = queueFileURL(),
           let data = try? Data(contentsOf: fileURL),
           let queue = try? decoder.decode([SharedImportEnvelope].self, from: data) {
            return queue
        }
        guard let data = defaults?.data(forKey: SharedImportStoreConfig.queueKey) else { return [] }
        return (try? decoder.decode([SharedImportEnvelope].self, from: data)) ?? []
    }

    private func save(_ queue: [SharedImportEnvelope]) {
        guard let data = try? encoder.encode(queue) else { return }
        if let fileURL = queueFileURL() {
            try? data.write(to: fileURL, options: .atomic)
        }
        defaults?.set(data, forKey: SharedImportStoreConfig.queueKey)
        defaults?.synchronize()
    }

    private func queueFileURL() -> URL? {
        SharedImportStoreConfig.containerURL()?.appendingPathComponent(SharedImportStoreConfig.queueFileName)
    }
}
