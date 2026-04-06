import SwiftUI
import UniformTypeIdentifiers

struct RoutesSettingsView: View {
    @EnvironmentObject private var appModel: AppModel
    @State private var showingImporter = false
    @State private var selectedItem: RouteHistoryItem?

    var body: some View {
        List {
            Section("Import") {
                Button("Import GPX") {
                    showingImporter = true
                }
            }

            Section("Recent") {
                if appModel.routeHistoryItems.isEmpty {
                    Text("No recent routes or destinations yet.")
                        .foregroundStyle(.secondary)
                } else {
                    ForEach(appModel.routeHistoryItems) { item in
                        NavigationLink(value: item.id) {
                            VStack(alignment: .leading, spacing: 4) {
                                Text(item.title)
                                Text(item.subtitle)
                                    .font(.subheadline)
                                    .foregroundStyle(.secondary)
                                Text(item.sourceLabel)
                                    .font(.caption)
                                    .foregroundStyle(.secondary)
                            }
                        }
                    }
                }
            }
        }
        .navigationTitle("Routes")
        .navigationDestination(for: String.self) { itemID in
            if let item = appModel.routeHistoryItems.first(where: { $0.id == itemID }) {
                RouteDetailView(
                    item: item,
                    onOpen: { open(item) },
                    onStart: { open(item); },
                    onDismiss: { selectedItem = nil }
                )
            }
        }
        .fileImporter(
            isPresented: $showingImporter,
            allowedContentTypes: [.xml, .item],
            allowsMultipleSelection: false
        ) { result in
            guard case let .success(urls) = result, let url = urls.first else { return }
            Task {
                await appModel.importGpxFile(from: url)
                appModel.recordPlannedPreview(source: .gpxImport, sourceLabel: "GPX")
            }
        }
    }

    private func open(_ item: RouteHistoryItem) {
        if let package = item.routePackage {
            let alternative = RouteAlternative(
                id: UUID(),
                title: item.title,
                subtitle: item.subtitle,
                distanceMeters: Int(package.summary.totalDistanceMeters.rounded()),
                durationSeconds: package.summary.estimatedDurationSeconds,
                normalizedPackage: package
            )
            appModel.preview = RoutePreviewModel(
                alternatives: [alternative],
                selectedAlternativeID: alternative.id,
                routeIdentifier: package.routeIdentifier,
                routeRevision: package.revision,
                planningNotice: item.sourceLabel
            )
        }
    }
}
