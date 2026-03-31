import SwiftUI

struct DeviceView: View {
    @EnvironmentObject private var appModel: AppModel

    var body: some View {
        NavigationStack {
            List {
                Section("BLE session") {
                    Text("Connection: \(appModel.bleService.sessionState.connectionState.rawValue)")
                    Text("Route sync: \(appModel.bleService.sessionState.routeSyncState.rawValue)")
                    Text("Pending route: \(appModel.bleService.sessionState.pendingRouteIdentifier ?? "None")")
                    Text("Pending revision: \(appModel.bleService.sessionState.pendingRouteRevision.map(String.init) ?? "0")")
                    Text("Active route: \(appModel.bleService.sessionState.activeRouteIdentifier ?? "None")")
                    Text("Active revision: \(appModel.bleService.sessionState.activeRouteRevision.map(String.init) ?? "0")")
                    Text("Active checksum: \(appModel.bleService.sessionState.activeRouteChecksumHex ?? "None")")
                    Text("Last sync: \(appModel.bleService.sessionState.lastSyncResult)")
                }

                Section("Transfer") {
                    Text("Retry fault armed: \(appModel.bleService.sessionState.retryableInterruptionArmed ? "Yes" : "No")")
                    Text("Armed fault: \(appModel.bleService.sessionState.armedFaultInjectionMode?.displayName ?? "None")")
                    if let transfer = appModel.bleService.sessionState.transferProgress {
                        Text("Transfer id: \(transfer.transferIdentifier)")
                        Text("Kind: \(transfer.messageKind)")
                        Text("Payload: \(transfer.payloadBytes) B")
                        Text("Chunks: \(transfer.acknowledgedChunks)/\(transfer.totalChunks) @ \(transfer.chunkSizeBytes) B")
                        Text("Progress: \(transfer.percentComplete)%")
                        Text("Retries: \(transfer.retryCount)")
                        Text("Checksum: \(transfer.checksumHex)")
                        Text("Resume chunk: \(transfer.resumeChunkIndex.map { String($0 + 1) } ?? "Complete")")
                        Text("Last transfer error: \(transfer.lastError ?? "None")")
                    } else {
                        Text("No active transfer")
                    }
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
                    Button("Arm retryable interruption") {
                        appModel.armRetryableInterruptionOnNextTransfer()
                    }
                    Button("Arm write failure") {
                        appModel.armWriteFailureOnNextTransfer()
                    }
                    Button("Arm disconnect after chunk") {
                        appModel.armDisconnectAfterNextChunkWrite()
                    }
                    Button("Arm drop next inbound status") {
                        appModel.armDropNextInboundStatus()
                    }
                    Button("Resume pending transfer") {
                        Task { await appModel.resumePendingTransfer() }
                    }
                    Button("Clear active route") {
                        Task { await appModel.clearActiveRoute() }
                    }
                    Button("Request reroute from rider location") {
                        Task { await appModel.handleRerouteRequest() }
                    }
                }
            }
            .navigationTitle("Device")
        }
    }
}
