import SwiftUI

struct LaunchView: View {
    @EnvironmentObject private var appModel: AppModel

    var body: some View {
        NavigationStack {
            List {
                Section("Connection") {
                    Text("BLE state: \(appModel.bleService.sessionState.connectionState.rawValue)")
                    Text("Last device: \(appModel.bleService.sessionState.lastDeviceName ?? "None")")
                    Button("Connect to device") {
                        Task { await appModel.connectToDevice() }
                    }
                }

                Section("Session") {
                    Text("Provider: \(appModel.selectedProviderID.displayName)")
                    Text("Route: \(appModel.activeSession.routeIdentifier ?? "None")")
                    Text("Sync: \(appModel.bleService.sessionState.routeSyncState.rawValue)")
                    Text("Planning mode: \(appModel.settings.preferLiveHslRouting ? "Live HSL" : "Sample HSL")")
                }

                if let notice = appModel.preview.planningNotice {
                    Section("Planning notice") {
                        Text(notice)
                            .foregroundStyle(.secondary)
                    }
                }
            }
            .navigationTitle("Companion")
        }
    }
}
