import SwiftUI

/// State-only descriptor of the "Paired device" section, derived from
/// `DeviceManager.pairedPeripheral` + the live BLE connection state. Keeping the
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
            gpsSourceSection
            transferSection
#if DEBUG
            diagnosticsSection
#endif
        }
        .navigationTitle(T.string("settings.device.title"))
        .alert(T.string("settings.device.forget.confirmTitle"), isPresented: $showForgetAlert) {
            Button(T.string("common.cancel.role"), role: .cancel) {}
            Button(T.string("common.forget"), role: .destructive) {
                appModel.deviceManager.forgetPairedDevice()
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
            get: { appModel.deviceManager.pairingState != .idle },
            set: { newValue in
                if !newValue {
                    appModel.deviceManager.pairingState = .idle
                }
            }
        )
    }

    private var pairedDeviceSection: some View {
        Section(T.string("settings.device.pairedDevice.section")) {
            switch DeviceSettingsSectionDescriptor.from(
                record: appModel.deviceManager.pairedPeripheral,
                connectionState: appModel.bleService.sessionState.connectionState
            ) {
            case .callToAction(let title):
                Button(title) {
                    appModel.deviceManager.beginPairingFlow()
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
        Section(T.string("settings.device.connection.section")) {
            Text(T.string("settings.device.connection.state",
                          ["state": .string(appModel.bleService.sessionState.connectionState.rawValue)]))
            Text(T.string("settings.device.connection.sync",
                          ["state": .string(appModel.bleService.sessionState.routeSyncState.rawValue)]))
            Text(T.string("settings.device.connection.lastDevice",
                          ["name": .string(appModel.bleService.sessionState.lastDeviceName ?? "—")]))
            // `lastSyncResult` carries the most recent transition string —
            // including BLE permission errors, scan timeouts, and connect
            // failures. Surfacing it here is what makes Reconnect feel
            // responsive when scans don't find the peripheral.
            Text(T.string("settings.device.connection.status",
                          ["status": .string(appModel.bleService.sessionState.lastSyncResult)]))
                .font(.footnote)
                .foregroundStyle(.secondary)
        }
    }

    private var gpsSourceSection: some View {
        Section("GPS Source") {
            Picker("GPS Source", selection: Binding(
                get: { appModel.deviceManager.gpsSource },
                set: { appModel.deviceManager.gpsSource = $0 }
            )) {
                Text("Internal (NEO-6M)").tag(GpsSourceSelection.internal)
                Text("Phone GPS").tag(GpsSourceSelection.phone)
            }
            .pickerStyle(.inline)
            .onChange(of: appModel.deviceManager.gpsSource) { newValue in
                appModel.deviceManager.handleGpsSourceChange(to: newValue)
            }
            if appModel.deviceManager.gpsSource == .phone {
                HStack {
                    Text("Status")
                    Spacer()
                    if appModel.deviceManager.isPhoneGpsForwarding {
                        Label("Active", systemImage: "location.fill")
                            .foregroundColor(.green)
                    } else {
                        Label("Inactive", systemImage: "location.slash")
                            .foregroundColor(.red)
                    }
                }
            }
        }
    }

    private var transferSection: some View {
        Section(T.string("settings.device.transfer.section")) {
            Text(T.string("settings.device.transfer.pendingRoute",
                          ["id": .string(appModel.bleService.sessionState.pendingRouteIdentifier ?? "—")]))
            Text(T.string("settings.device.transfer.activeRoute",
                          ["id": .string(appModel.bleService.sessionState.activeRouteIdentifier ?? "—")]))
            Text(T.string("settings.device.transfer.lastSync",
                          ["result": .string(appModel.bleService.sessionState.lastSyncResult)]))
            Button(T.string("settings.device.transfer.resume")) {
                Task { await appModel.resumePendingTransfer() }
            }
            Button(T.string("settings.device.transfer.clearActive"), role: .destructive) {
                Task { await appModel.routeSyncService.clearActiveRoute() }
            }
        }
    }

#if DEBUG
    private var diagnosticsSection: some View {
        let model = appModel
        return Section(T.string("settings.device.diagnostics.section")) {
            Button(T.string("settings.device.diagnostics.armRetryable")) { model.armRetryableInterruptionOnNextTransfer() }
            Button(T.string("settings.device.diagnostics.armWriteFailure")) { model.armWriteFailureOnNextTransfer() }
            Button(T.string("settings.device.diagnostics.armDisconnect")) { model.armDisconnectAfterNextChunkWrite() }
            Button(T.string("settings.device.diagnostics.armDropStatus")) { model.armDropNextInboundStatus() }
        }
    }
#endif
}
