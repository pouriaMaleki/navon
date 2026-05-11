import SwiftUI
import UIKit

struct RoutingDiagnosticsView: View {
    @EnvironmentObject private var appModel: AppModel

    var body: some View {
        List {
            if appModel.routingDiagnosticsSessions.isEmpty {
                Text("No routing diagnostics recorded.")
                    .foregroundStyle(.secondary)
            } else {
                ForEach(appModel.routingDiagnosticsSessions) { session in
                    Section {
                        Text("\(session.events.count) events over \(durationText(session.durationMs))")
                            .foregroundStyle(.secondary)
                        Button("Copy debug info") {
                            UIPasteboard.general.string = session.debugPackageText
                        }
                        ShareLink(item: session.debugPackageText) {
                            Text("Share package")
                        }
                        Button("Delete", role: .destructive) {
                            appModel.dismissRoutingDiagnosticsSession(id: session.id)
                        }
                    } header: {
                        Text(sessionStartText(session.createdAtMs))
                    }
                }
            }
        }
        .navigationTitle("Routing Diagnostics")
    }

    private func sessionStartText(_ timestampMs: UInt64) -> String {
        let date = Date(timeIntervalSince1970: TimeInterval(timestampMs) / 1000)
        return date.formatted(date: .abbreviated, time: .shortened)
    }

    private func durationText(_ ms: UInt64) -> String {
        if ms >= 60_000 {
            return "\(ms / 60_000) min"
        }
        return "\((ms + 500) / 1000) sec"
    }
}
