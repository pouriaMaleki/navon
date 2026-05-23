import Foundation
import Combine
import os.log

@MainActor
final class DeviceManager: ObservableObject {
    @Published private(set) var pairedPeripheral: PairedPeripheralRecord?
    @Published var pairingState: PairingFlowState = .idle
    @Published var gpsSource: AppModel.GpsSourceSelection = .internal
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
    }

    func forgetPairedDevice() {
        pairedPeripheral = nil
        persistence.clearPairedPeripheral()
        pairingState = .idle
    }

    func beginPairingFlow() {
        pairingState = .instructions
    }

    func handleGpsSourceChange(to source: AppModel.GpsSourceSelection) {
        gpsSource = source
        switch source {
        case .internal:
            phoneGpsForwarder?.stopForwarding()
        case .phone:
            phoneGpsForwarder?.startForwarding()
        }
    }

    func prepareDeviceForPairing() async throws {
        try await bleService.scanAndConnectForPairing()
    }

    func completePairing(payload: PairingQrPayload) async throws {
        try await bleService.confirmPairing(secret: payload.ephemeralSecret)
        let info = try await bleService.connectToAdvertisedPeripheral()
        let record = PairedPeripheralRecord(
            name: info.name,
            identifier: info.identifier,
            firmwareVersion: payload.firmwareVersion ?? "unknown",
            pairedAt: Date(),
            androidIdentifier: payload.androidIdentifier
        )
        pairedPeripheral = record
        persistence.savePairedPeripheral(record)
        pairingState = .idle
    }

    func connectToDevice() async {
        if let paired = pairedPeripheral {
            await bleService.connectToPairedPeripheral(identifier: paired.identifier)
        }
    }

    func resumePendingTransfer() async {
        try? await bleService.resumePendingTransfer()
    }

    #if DEBUG
    private var deviceConnectedTestOverride: Bool?

    func replacePairedPeripheralForTesting(_ record: PairedPeripheralRecord?) {
        pairedPeripheral = record
    }

    func replaceDeviceConnectedForTesting(_ value: Bool?) {
        deviceConnectedTestOverride = value
    }

    var isDeviceConnectedForCueSuppression: Bool {
        if let override = deviceConnectedTestOverride { return override }
        return isDeviceConnected
    }

    func armRetryableInterruptionOnNextTransfer() {
        bleService.armRetryableInterruptionOnNextTransfer()
    }

    func armWriteFailureOnNextTransfer() {
        bleService.armFaultInjection(.writeFailure)
    }

    func armDisconnectAfterNextChunkWrite() {
        bleService.armFaultInjection(.disconnectAfterChunkWrite)
    }

    func armDropNextInboundStatus() {
        bleService.armFaultInjection(.dropNextInboundStatus)
    }
    #endif
}
