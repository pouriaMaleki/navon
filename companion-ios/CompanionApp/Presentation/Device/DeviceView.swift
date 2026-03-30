import SwiftUI

struct DeviceView: View {
    @EnvironmentObject private var appModel: AppModel

    var body: some View {
        NavigationStack {
            List {
                Section("BLE session") {
                    Text("Connection: \(appModel.bleService.sessionState.connectionState.rawValue)")
                    Text("Route sync: \(appModel.bleService.sessionState.routeSyncState.rawValue)")
                    Text("Active route: \(appModel.bleService.sessionState.activeRouteIdentifier ?? "None")")
                    Text("Active revision: \(appModel.bleService.sessionState.activeRouteRevision.map(String.init) ?? "0")")
                    Text("Last sync: \(appModel.bleService.sessionState.lastSyncResult)")
                }

                Section("Messages") {
                    Text("Outbound: \(appModel.bleService.sessionState.lastOutboundMessage?.debugSummary ?? "None")")
                    Text("Inbound: \(appModel.bleService.sessionState.lastInboundMessage?.debugSummary ?? "None")")
                    Text("Status code: \(appModel.bleService.sessionState.lastStatusCode?.rawValue ?? "none")")
                }

                Section("Actions") {
                    Button("Reconnect") {
                        Task { await appModel.connectToDevice() }
                    }
                    Button("Send route message") {
                        Task { await appModel.sendSelectedRoute() }
                    }
                    Button("Clear active route") {
                        Task { await appModel.clearActiveRoute() }
                    }
                    Button("Simulate reroute request") {
                        Task { await appModel.handleDemoRerouteRequest() }
                    }
                }
            }
            .navigationTitle("Device")
        }
    }
}
