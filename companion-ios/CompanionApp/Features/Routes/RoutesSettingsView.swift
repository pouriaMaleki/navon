import SwiftUI
import UniformTypeIdentifiers

struct RoutesSettingsView: View {
    @EnvironmentObject private var appModel: AppModel
    @State private var showingImporter = false

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
                        HStack(spacing: 12) {
                            Button {
                                appModel.activateRouteHistoryItem(item, startImmediately: false)
                            } label: {
                                VStack(alignment: .leading, spacing: 4) {
                                    Text(item.title)
                                    Text(item.subtitle)
                                        .font(.subheadline)
                                        .foregroundStyle(.secondary)
                                    Text(item.sourceLabel)
                                        .font(.caption)
                                        .foregroundStyle(.secondary)
                                }
                                .frame(maxWidth: .infinity, alignment: .leading)
                                .contentShape(Rectangle())
                            }
                            .buttonStyle(.plain)

                            NavigationLink(value: item.id) {
                                Image(systemName: "info.circle")
                                    .foregroundStyle(.secondary)
                                    .font(.system(size: 18, weight: .semibold))
                                    .frame(width: 28, height: 28)
                            }
                            .accessibilityLabel("Route details")
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
                    onOpen: { appModel.activateRouteHistoryItem(item, startImmediately: false) },
                    onStart: { appModel.activateRouteHistoryItem(item, startImmediately: true) },
                    onDismiss: { appModel.dismissRouteHistoryItem(id: item.id) }
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
                appModel.homePreviewRequestID = UUID()
            }
        }
    }
}
