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
                                Text(alternative.normalizedPackage.summaryLine)
                                    .font(.caption)
                                Text("\(alternative.normalizedPackage.geometryPointCount) geometry points • \(alternative.normalizedPackage.maneuverCount) maneuvers")
                                    .font(.caption)
                                    .foregroundStyle(.secondary)
                            }
                        }
                    }
                }

                Section("Send") {
                    Text("Route id: \(appModel.preview.routeIdentifier ?? "None")")
                    Text("Revision: \(appModel.preview.routeRevision.map(String.init) ?? "0")")
                    if let selected = appModel.preview.selectedAlternative?.normalizedPackage {
                        Text("Provenance: \(selected.provenance.providerID.displayName)")
                        Text("Source: \(selected.provenance.sourceReference ?? "None")")
                    }
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
