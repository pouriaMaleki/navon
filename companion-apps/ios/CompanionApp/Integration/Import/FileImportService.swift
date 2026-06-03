import Foundation

struct FileImportResult {
    let preview: RoutePreviewModel
    let providerID: RouteProviderID
    let suggestedTitle: String
    let geometryOrigin: CoordinatePoint?
    let geometryDestination: CoordinatePoint?
}

/// Handles security-scoped file access and provider-specific parsing
/// for GPX, FIT, and TCX route files. State updates are left to the caller.
enum FileImportService {

    static func importGpxFile(from url: URL, adapter: GpxRoutingAdapter) throws -> FileImportResult {
        let accessGranted = url.startAccessingSecurityScopedResource()
        defer {
            if accessGranted { url.stopAccessingSecurityScopedResource() }
        }
        let data = try Data(contentsOf: url)
        let preview = try adapter.importFile(named: url.lastPathComponent, data: data)
        return FileImportResult(
            preview: preview,
            providerID: .gpxImport,
            suggestedTitle: url.deletingPathExtension().lastPathComponent,
            geometryOrigin: preview.selectedAlternative?.normalizedPackage.geometry.first,
            geometryDestination: preview.selectedAlternative?.normalizedPackage.geometry.last
        )
    }

    static func importSampleFile(
        from url: URL,
        providerID: RouteProviderID,
        adapter: SampleRoutingAdapter,
        origin: CoordinatePoint,
        preferredTitle: String?
    ) throws -> FileImportResult {
        let accessGranted = url.startAccessingSecurityScopedResource()
        defer {
            if accessGranted { url.stopAccessingSecurityScopedResource() }
        }
        let data = try Data(contentsOf: url)
        let preview = try adapter.importFile(named: url.lastPathComponent, data: data, origin: origin)
        return FileImportResult(
            preview: preview,
            providerID: providerID,
            suggestedTitle: preferredTitle ?? url.deletingPathExtension().lastPathComponent,
            geometryOrigin: preview.selectedAlternative?.normalizedPackage.geometry.first,
            geometryDestination: preview.selectedAlternative?.normalizedPackage.geometry.last
        )
    }
}
