import SwiftUI

struct DeviceSettingsView: View {
    @EnvironmentObject private var appModel: AppModel
    @State private var isConnecting = false

    var body: some View {
        List {
            Section("Connection") {
                Text("State: \(appModel.bleService.sessionState.connectionState.rawValue)")
                Text("Sync: \(appModel.bleService.sessionState.routeSyncState.rawValue)")
                Text("Last device: \(appModel.bleService.sessionState.lastDeviceName ?? "None")")
                // `lastSyncResult` carries the most recent transition string —
                // including BLE permission errors, scan timeouts, and connect
                // failures. Surfacing it here is what makes Reconnect feel
                // responsive when scans don't find the peripheral.
                Text("Status: \(appModel.bleService.sessionState.lastSyncResult)")
                    .font(.footnote)
                    .foregroundStyle(.secondary)
                Button(isConnecting ? "Reconnecting…" : "Reconnect") {
                    isConnecting = true
                    Task {
                        await appModel.connectToDevice()
                        isConnecting = false
                    }
                }
                .disabled(isConnecting)
            }

            Section("Transfer") {
                Text("Pending route: \(appModel.bleService.sessionState.pendingRouteIdentifier ?? "None")")
                Text("Active route: \(appModel.bleService.sessionState.activeRouteIdentifier ?? "None")")
                Text("Last sync: \(appModel.bleService.sessionState.lastSyncResult)")
                Button("Resume pending transfer") {
                    Task { await appModel.resumePendingTransfer() }
                }
                Button("Clear active route", role: .destructive) {
                    Task { await appModel.clearActiveRoute() }
                }
            }

            Section("Diagnostics") {
                Button("Arm retryable interruption") { appModel.armRetryableInterruptionOnNextTransfer() }
                Button("Arm write failure") { appModel.armWriteFailureOnNextTransfer() }
                Button("Arm disconnect after chunk") { appModel.armDisconnectAfterNextChunkWrite() }
                Button("Arm drop next inbound status") { appModel.armDropNextInboundStatus() }
            }
        }
        .navigationTitle("Device")
    }
}
