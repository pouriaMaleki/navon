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
                Section(T.string("routeDetail.section.route")) {
                    Text(route.summaryLine)
                    Text(T.string(
                        "routeDetail.maneuverCount",
                        ["count": .number(Double(route.maneuverCount))]
                    ))
                    Text(T.string(
                        "routeDetail.geometryPoints",
                        ["count": .number(Double(route.geometryPointCount))]
                    ))
                }
            }

            Section(T.string("routeDetail.section.actions")) {
                Button(T.string("routeDetail.open")) {
                    onOpen()
                }
                Button(T.string("home.start")) {
                    onStart()
                }
                Button(T.string("common.dismiss"), role: .destructive) {
                    onDismiss()
                    dismiss()
                }
            }
        }
        .navigationTitle(T.string("routeDetail.title"))
        .navigationBarTitleDisplayMode(.inline)
        .onChange(of: appModel.homePreviewRequestID) { _, _ in
            dismiss()
        }
    }
}
