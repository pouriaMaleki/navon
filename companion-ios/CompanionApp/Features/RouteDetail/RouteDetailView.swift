import SwiftUI

struct RouteDetailView: View {
    @EnvironmentObject private var appModel: AppModel
    @Environment(\.dismiss) private var dismiss
    let item: RouteHistoryItem
    let onOpen: () -> Void
    let onStart: () -> Void
    let onDismiss: () -> Void

    var body: some View {
        List {
            Section {
                Text(item.title)
                    .font(.title2.weight(.semibold))
                Text(item.sourceLabel)
                    .font(.subheadline)
                    .foregroundStyle(.secondary)
                Text(item.subtitle)
                    .foregroundStyle(.secondary)
            }

            if let route = item.routePackage {
                Section("Route") {
                    Text(route.summaryLine)
                    Text("Maneuvers: \(route.maneuverCount)")
                    Text("Geometry points: \(route.geometryPointCount)")
                }
            }

            Section("Actions") {
                Button("Open") {
                    onOpen()
                }
                Button("Start") {
                    onStart()
                }
                Button("Dismiss", role: .destructive) {
                    onDismiss()
                    dismiss()
                }
            }
        }
        .navigationTitle("Route")
        .navigationBarTitleDisplayMode(.inline)
        .onChange(of: appModel.homePreviewRequestID) { _, _ in
            dismiss()
        }
    }
}
