import SwiftUI
import UIKit

struct ImportDiagnosticsView: View {
    @EnvironmentObject private var shareImportService: ShareImportService

    var body: some View {
        List {
            if shareImportService.importDiagnosticsEntries.isEmpty {
                Text(T.string("settings.importDiagnostics.empty"))
                    .foregroundStyle(.secondary)
            } else {
                ForEach(shareImportService.importDiagnosticsEntries) { entry in
                    Section(entry.title) {
                        Text(entry.subtitle)
                            .foregroundStyle(.secondary)
                        Text(entry.envelope.classification.rawValue)
                            .font(.caption)
                            .foregroundStyle(.secondary)
                        if let sourceApp = entry.envelope.sourceApplication {
                            Text(sourceApp)
                                .font(.caption)
                                .foregroundStyle(.secondary)
                        }
                        Button(T.string("importDiagnostics.copyDebugInfo")) {
                            UIPasteboard.general.string = entry.debugPackageText
                        }
                        ShareLink(item: entry.debugPackageText, subject: Text(T.string("importDiagnostics.shareSubject"))) {
                            Text(T.string("importDiagnostics.sharePackage"))
                        }
                        Button(T.string("importDiagnostics.retry")) {
                            Task {
                                await shareImportService.retrySharedImport(entry)
                            }
                        }
                        Button(T.string("common.dismiss"), role: .destructive) {
                            shareImportService.dismissImportDiagnosticsEntry(id: entry.id)
                        }
                    }
                }
            }
        }
        .navigationTitle(T.string("settings.importDiagnostics.title"))
    }
}
