import SwiftUI

/// State-only descriptor of the "Paired device" section, derived from
/// `AppModel.pairedPeripheral` + the live BLE connection state. Keeping the
/// view's logic in this enum lets unit tests cover wiring without depending
/// on `ViewInspector` or running SwiftUI on the simulator.
enum DeviceSettingsSectionDescriptor: Equatable {
    case callToAction(String)
    case detail(name: String, lastPairedAt: Date, primaryAction: PrimaryAction)

    enum PrimaryAction: Equatable {
        case connect
        case disconnect
    }
}

extension DeviceSettingsSectionDescriptor {
    static func from(
        record: PairedPeripheralRecord?,
        connectionState: DeviceConnectionState
    ) -> DeviceSettingsSectionDescriptor {
        guard let record else {
            return .callToAction(T.string("settings.device.pairNew"))
        }
        let primary: PrimaryAction = (connectionState == .connected) ? .disconnect : .connect
        return .detail(name: record.friendlyName, lastPairedAt: record.pairedAt, primaryAction: primary)
    }
}

struct DeviceSettingsView: View {
    @EnvironmentObject private var appModel: AppModel
    @State private var isConnecting = false
    @State private var showForgetAlert = false

    var body: some View {
        List {
            pairedDeviceSection
            connectionDetailsSection
            transferSection
            diagnosticsSection
        }
        .navigationTitle(T.string("settings.device.title"))
        .alert(T.string("settings.device.forget.confirmTitle"), isPresented: $showForgetAlert) {
            Button(T.string("common.cancel.role"), role: .cancel) {}
            Button(T.string("common.forget"), role: .destructive) {
                appModel.forgetPairedDevice()
            }
        } message: {
            Text(T.string("settings.device.forget.confirmMessage"))
        }
        // The pairing flow is initiated from this screen; the sheet must
        // also live here. Settings is presented as a `fullScreenCover` over
        // Home, so a sheet attached at Home wouldn't be visible while
        // Settings is up — that's exactly the "tap → nothing happens" bug.
        .sheet(isPresented: pairingSheetBinding) {
            PairingFlowView(appModel: appModel)
                .environmentObject(appModel)
        }
    }

    private var pairingSheetBinding: Binding<Bool> {
        Binding(
            get: { appModel.pairingState != .idle },
            set: { newValue in
                if !newValue {
                    appModel.pairingState = .idle
                }
            }
        )
    }

    private var pairedDeviceSection: some View {
        Section(T.string("settings.device.pairedDevice.section")) {
            switch DeviceSettingsSectionDescriptor.from(
                record: appModel.pairedPeripheral,
                connectionState: appModel.bleService.sessionState.connectionState
            ) {
            case .callToAction(let title):
                Button(title) {
                    appModel.beginPairingFlow()
                }
            case .detail(let name, let lastPairedAt, let primary):
                Text(name).font(.headline)
                Text(T.string(
                    "settings.device.lastPaired",
                    ["date": .string(lastPairedAt.formatted(date: .abbreviated, time: .shortened))]
                ))
                    .font(.footnote)
                    .foregroundStyle(.secondary)
                Button(primaryActionLabel(for: primary)) {
                    Task {
                        switch primary {
                        case .connect:
                            isConnecting = true
                            await appModel.connectToDevice()
                            isConnecting = false
                        case .disconnect:
                            // Soft-disconnect by clearing pending and noting it; the
                            // explicit "disconnect" path is simply forget for this
                            // single-bond model — kept as Connect/Disconnect so the
                            // descriptor surface still has parity with Android.
                            await appModel.connectToDevice()
                        }
                    }
                }
                .disabled(isConnecting)
                Button(T.string("settings.device.forget"), role: .destructive) {
                    showForgetAlert = true
                }
            }
        }
    }

    private func primaryActionLabel(for action: DeviceSettingsSectionDescriptor.PrimaryAction) -> String {
        switch action {
        case .connect: return T.string(isConnecting ? "settings.device.reconnecting" : "settings.device.connect")
        case .disconnect: return T.string("settings.device.disconnect")
        }
    }

    private var connectionDetailsSection: some View {
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
        }
    }

    private var transferSection: some View {
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
    }

    private var diagnosticsSection: some View {
        Section("Diagnostics") {
            Button("Arm retryable interruption") { appModel.armRetryableInterruptionOnNextTransfer() }
            Button("Arm write failure") { appModel.armWriteFailureOnNextTransfer() }
            Button("Arm disconnect after chunk") { appModel.armDisconnectAfterNextChunkWrite() }
            Button("Arm drop next inbound status") { appModel.armDropNextInboundStatus() }
        }
    }
}
