import SwiftUI

struct RoutePreviewView: View {
    @EnvironmentObject private var appModel: AppModel

    var body: some View {
        NavigationStack {
            List {
                Section("Alternatives") {
                    if appModel.preview.alternatives.isEmpty {
                        Text("No planned route yet")
                            .foregroundStyle(.secondary)
                    } else {
                        ForEach(appModel.preview.alternatives) { alternative in
                            VStack(alignment: .leading, spacing: 4) {
                                Text(alternative.title)
                                    .font(.headline)
                                Text(alternative.subtitle)
                                    .foregroundStyle(.secondary)
                                Text("\(alternative.distanceMeters) m • \(alternative.durationSeconds / 60) min")
                                    .font(.caption)
                            }
                        }
                    }
                }

                Section("Send") {
                    Text("Route id: \(appModel.preview.routeIdentifier ?? "None")")
                    Text("Revision: \(appModel.preview.routeRevision.map(String.init) ?? "0")")
                    Button("Send to device") {
                        Task { await appModel.sendSelectedRoute() }
                    }
                    .disabled(appModel.preview.routeIdentifier == nil)
                }
            }
            .navigationTitle("Route preview")
        }
    }
}
