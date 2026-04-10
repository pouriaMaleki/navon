import SwiftUI

struct SettingsHubView: View {
    @EnvironmentObject private var appModel: AppModel
    @Environment(\.dismiss) private var dismiss

    var body: some View {
        NavigationStack {
            List {
                NavigationLink("Routes") {
                    RoutesSettingsView()
                }
                NavigationLink("Device") {
                    DeviceSettingsView()
                }
                NavigationLink("Route Planner") {
                    RoutePlannerSettingsView()
                }
                NavigationLink("Import Diagnostics") {
                    ImportDiagnosticsView()
                }
            }
            .navigationTitle("Settings")
            .toolbar {
                ToolbarItem(placement: .topBarLeading) {
                    Button("Done") { dismiss() }
                }
            }
        }
    }
}
