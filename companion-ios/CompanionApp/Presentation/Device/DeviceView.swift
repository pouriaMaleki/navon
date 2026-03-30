import SwiftUI

struct DeviceView: View {
    @EnvironmentObject private var appModel: AppModel

    var body: some View {
        NavigationStack {
            List {
                Section("BLE session") {
                    Text("Connection: \(appModel.bleService.sessionState.connectionState.rawValue)")
                    Text("Route sync: \(appModel.bleService.sessionState.routeSyncState.rawValue)")
                    Text("Last sync: \(appModel.bleService.sessionState.lastSyncResult)")
                }

                Section("Actions") {
                    Button("Reconnect") {
                        Task { await appModel.connectToDevice() }
                    }
                    Button("Trigger demo reroute") {
                        Task { await appModel.handleDemoRerouteRequest() }
                    }
                }
            }
            .navigationTitle("Device")
        }
    }
}
