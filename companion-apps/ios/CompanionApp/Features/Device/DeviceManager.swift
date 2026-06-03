import Foundation
import Combine
import os.log

/// Pairing-flow log channel. Filter Console.app with
/// `subsystem:app.navon.bike category:pairing` to follow
/// each step (begin → scan → connect → write → persist).
let pairingLog = Logger(subsystem: "app.navon.bike", category: "pairing")

enum GpsSourceSelection: String, CaseIterable {
    case `internal`
    case phone
}

@MainActor
final class DeviceManager: ObservableObject {
    @Published private(set) var pairedPeripheral: PairedPeripheralRecord?
    @Published var pairingState: PairingFlowState = .idle
    @Published var gpsSource: GpsSourceSelection = .internal
    @Published private(set) var isPhoneGpsForwarding: Bool = false

    private let bleService: BleRouteSyncService
    private let persistence: CompanionPersistence
    private let locationService: CoreLocationService
    private var phoneGpsForwarder: PhoneGpsForwarder?
    private var cancellables = Set<AnyCancellable>()

    var isDeviceConnected: Bool {
        bleService.sessionState.connectionState == .connected
    }

    init(
        bleService: BleRouteSyncService,
        persistence: CompanionPersistence,
        locationService: CoreLocationService
    ) {
        self.bleService = bleService
        self.persistence = persistence
        self.locationService = locationService
        self.phoneGpsForwarder = PhoneGpsForwarder(
            bleClient: bleService.bluetoothClient,
            locationService: locationService
        )
        self.pairedPeripheral = persistence.loadPairedPeripheral()

        phoneGpsForwarder?.$isForwarding
            .receive(on: DispatchQueue.main)
            .assign(to: \.isPhoneGpsForwarding, on: self)
            .store(in: &cancellables)

        // App-launch auto-reconnect: if the user was previously paired,
        // try to connect once in the background.
        if let paired = pairedPeripheral {
            Task { [weak self] in
                pairingLog.notice("DeviceManager.init — auto-reconnect to paired peripheral [\(paired.identifier, privacy: .public)]")
                await self?.bleService.connectToPairedPeripheral(identifier: paired.identifier)
            }
        }
    }

    func stopPhoneGpsForwarding() {
        phoneGpsForwarder?.stop()
    }

    func forgetPairedDevice() {
        pairedPeripheral = nil
        persistence.clearPairedPeripheral()
        pairingState = .idle
    }

    func beginPairingFlow() {
        pairingLog.notice("beginPairingFlow tapped — pairingState → .instructions")
        pairingState = .instructions
    }

    func handleGpsSourceChange(to source: GpsSourceSelection) {
        gpsSource = source
        switch source {
        case .internal:
            phoneGpsForwarder?.stop()
        case .phone:
            phoneGpsForwarder?.start()
        }
    }

    /// Step the device into pairing mode before the camera opens.
    func prepareDeviceForPairing() async throws {
        pairingLog.notice("prepareDeviceForPairing — scan + connect + writePairingRequest")
        pairingState = .connecting
        do {
            _ = try await bleService.connectToAdvertisedPeripheral()
            try await bleService.writePairingRequest()
            pairingLog.notice("prepareDeviceForPairing OK — device should now show QR")
        } catch {
            pairingLog.error("prepareDeviceForPairing failed: \(error.localizedDescription, privacy: .public)")
            pairingState = .failed(error.localizedDescription)
            throw error
        }
    }

    /// Drive the pairing flow's BLE half: connect, write the OOB confirmation secret,
    /// and persist the bond on success. Auto-dismisses after 1.5 s on success.
    func completePairing(payload: PairingQrPayload) async {
        pairingLog.notice("completePairing — confirming (secret \(payload.ephemeralSecret.count, privacy: .public) B)")
        do {
            let info = try await bleService.connectToAdvertisedPeripheral()
            pairingLog.notice("completePairing — connection ready: \(info.name, privacy: .public) [\(info.identifier, privacy: .public)]")
            pairingState = .confirming
            try await bleService.writePairingConfirm(secret: payload.ephemeralSecret)
            pairingLog.notice("completePairing — pairing-confirm write OK")
            let record = PairedPeripheralRecord(
                identifier: info.identifier,
                friendlyName: info.name,
                pairedAt: Date()
            )
            persistence.savePairedPeripheral(record)
            pairedPeripheral = record
            pairingState = .succeeded
            try? await Task.sleep(nanoseconds: 1_500_000_000)
            if pairingState == .succeeded {
                pairingState = .idle
            }
        } catch {
            pairingLog.error("completePairing failed: \(error.localizedDescription, privacy: .public)")
            pairingState = .failed(error.localizedDescription)
        }
    }

    func connectToDevice() async {
        if let paired = pairedPeripheral {
            await bleService.connectToPairedPeripheral(identifier: paired.identifier)
            if bleService.sessionState.connectionState == .connected {
                return
            }
        }
        await bleService.scanForDevices()
        await bleService.connectToLastKnownDevice()
    }

    func resumePendingTransfer() async {
        try? await bleService.resumePendingTransfer()
    }

#if DEBUG
    func replacePairedPeripheralForTesting(_ record: PairedPeripheralRecord?) {
        pairedPeripheral = record
    }

    private var deviceConnectedTestOverride: Bool?

    func replaceDeviceConnectedForTesting(_ value: Bool?) {
        deviceConnectedTestOverride = value
    }
#endif

    /// True when the companion is actively connected to the ESP32 device.
    /// When the device is the on-screen UI, the phone goes silent (no
    /// cues) and the live activity is suppressed.
    var isDeviceConnectedForCueSuppression: Bool {
        #if DEBUG
        if let override = deviceConnectedTestOverride { return override }
        #endif
        return isDeviceConnected
    }
}
