import SwiftUI
import UIKit

struct ImportDiagnosticsView: View {
    @EnvironmentObject private var appModel: AppModel

    var body: some View {
        List {
            if appModel.importDiagnosticsEntries.isEmpty {
                Text(T.string("settings.importDiagnostics.empty"))
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
                        Button(T.string("importDiagnostics.copyDebugInfo")) {
                            UIPasteboard.general.string = entry.debugPackageText
                        }
                        ShareLink(item: entry.debugPackageText, subject: Text(T.string("importDiagnostics.shareSubject"))) {
                            Text(T.string("importDiagnostics.sharePackage"))
                        }
                        Button(T.string("importDiagnostics.retry")) {
                            Task {
                                await appModel.retrySharedImport(entry)
                            }
                        }
                        Button(T.string("common.dismiss"), role: .destructive) {
                            appModel.dismissImportDiagnosticsEntry(id: entry.id)
                        }
                    }
                }
            }
        }
        .navigationTitle(T.string("settings.importDiagnostics.title"))
    }
}
