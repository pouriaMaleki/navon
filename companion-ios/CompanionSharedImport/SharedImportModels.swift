import Foundation

enum SharedImportRawKind: String, Codable, Equatable {
    case url
    case plainText
    case file
    case multipleItems
    case unknown
}

enum SharedImportClassification: String, Codable, Equatable {
    case googleMapsLocationLink
    case genericLocationLink
    case gpxFile
    case genericXMLFile
    case fitFile
    case tcxFile
    case plainCoordinates
    case unsupportedUnknown
}

enum SharedImportDisposition: String, Codable, Equatable {
    case directHomePreview
    case routeDetailReview
    case diagnosticsOnly
}

struct SharedImportEnvelope: Identifiable, Codable, Equatable {
    var id: String
    var sourceApplication: String?
    var receivedAt: Date
    var rawKind: SharedImportRawKind
    var mimeType: String?
    var uniformTypeIdentifier: String?
    var fileName: String?
    var fileSizeBytes: Int?
    var originalText: String?
    var originalURL: String?
    var storedFilePath: String?
    var classification: SharedImportClassification
    var disposition: SharedImportDisposition
    var note: String?
    var debugTrail: [String]?
}

struct ImportDiagnosticsEntry: Identifiable, Codable, Equatable {
    var id: String
    var envelope: SharedImportEnvelope
    var createdAt: Date

    var title: String {
        if let fileName = envelope.fileName, !fileName.isEmpty {
            return fileName
        }
        if let url = envelope.originalURL, let host = URL(string: url)?.host, !host.isEmpty {
            return host
        }
        if let sourceApplication = envelope.sourceApplication, !sourceApplication.isEmpty {
            return sourceApplication
        }
        return envelope.classification.rawValue
    }

    var subtitle: String {
        if let note = envelope.note, !note.isEmpty {
            return note
        }
        if let text = envelope.originalText?.trimmingCharacters(in: .whitespacesAndNewlines), !text.isEmpty {
            return String(text.prefix(120))
        }
        if let url = envelope.originalURL, !url.isEmpty {
            return String(url.prefix(120))
        }
        return "Unsupported shared item"
    }

    var debugPackageText: String {
        let encoder = JSONEncoder()
        encoder.outputFormatting = [.prettyPrinted, .sortedKeys]
        let json = (try? encoder.encode(envelope)).flatMap { String(data: $0, encoding: .utf8) } ?? "{}"
        return json
    }
}
