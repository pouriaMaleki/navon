import SwiftUI

struct SettingsView: View {
    @EnvironmentObject private var appModel: AppModel

    var body: some View {
        NavigationStack {
            List {
                Section("Provider shell") {
                    ForEach(appModel.providerOptions) { provider in
                        HStack {
                            Text(provider.displayName)
                            Spacer()
                            Text(provider.isAvailableInV1 ? "Active" : "Coming soon")
                                .foregroundStyle(provider.isAvailableInV1 ? .green : .secondary)
                        }
                    }
                }

                Section("Diagnostics") {
                    let diagnostics = appModel.diagnosticsStore.diagnostics
                    Text("Provider: \(diagnostics.providerName)")
                    Text("Route: \(diagnostics.routeIdentifier)")
                    Text("Revision: \(diagnostics.routeRevision)")
                    Text("BLE: \(diagnostics.bleState)")
                    Text("Sync: \(diagnostics.lastSyncResult)")
                    Text("Reroute: \(diagnostics.lastRerouteOutcome)")
                }

                Section("Sync contract") {
                    Text("Last outbound: \(appModel.bleService.sessionState.lastOutboundMessage?.kindLabel ?? "none")")
                    Text("Last inbound: \(appModel.bleService.sessionState.lastInboundMessage?.kindLabel ?? "none")")
                }
            }
            .navigationTitle("Settings")
            .onAppear { appModel.refreshDiagnostics() }
        }
    }
}
