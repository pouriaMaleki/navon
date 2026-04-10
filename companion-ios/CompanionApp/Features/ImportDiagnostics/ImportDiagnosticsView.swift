import SwiftUI
import UIKit

struct ImportDiagnosticsView: View {
    @EnvironmentObject private var appModel: AppModel

    var body: some View {
        List {
            if appModel.importDiagnosticsEntries.isEmpty {
                Text("No unsupported shared items yet.")
                    .foregroundStyle(.secondary)
            } else {
                ForEach(appModel.importDiagnosticsEntries) { entry in
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
                        Button("Copy debug info") {
                            UIPasteboard.general.string = entry.debugPackageText
                        }
                        ShareLink(item: entry.debugPackageText, subject: Text("Companion import diagnostics")) {
                            Text("Share debug package")
                        }
                        Button("Retry import") {
                            Task {
                                await appModel.retrySharedImport(entry)
                            }
                        }
                        Button("Dismiss", role: .destructive) {
                            appModel.dismissImportDiagnosticsEntry(id: entry.id)
                        }
                    }
                }
            }
        }
        .navigationTitle("Import Diagnostics")
    }
}
