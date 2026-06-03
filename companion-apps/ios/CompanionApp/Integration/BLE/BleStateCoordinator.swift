import Foundation
import Combine

/// Observes BLE session state and dispatches side effects: GPS stop on disconnect,
/// reroute requests from device, and diagnostics refresh.
/// Extracted from AppModel.bindBleState().
@MainActor
final class BleStateCoordinator {
    private let bleService: BleRouteSyncService
    private let deviceManager: DeviceManager
    private let onRefreshDiagnostics: () -> Void
    private let onRerouteRequest: (CoordinatePoint, String) async -> Void

    private var cancellables = Set<AnyCancellable>()
    private var lastHandledRerouteSignature: String?

    init(
        bleService: BleRouteSyncService,
        deviceManager: DeviceManager,
        onRefreshDiagnostics: @escaping () -> Void,
        onRerouteRequest: @escaping (CoordinatePoint, String) async -> Void
    ) {
        self.bleService = bleService
        self.deviceManager = deviceManager
        self.onRefreshDiagnostics = onRefreshDiagnostics
        self.onRerouteRequest = onRerouteRequest
        bind()
    }

    /// Forwards `bleService.objectWillChange` to the given publisher so SwiftUI
    /// views observing the parent re-render on BLE state transitions.
    func forwardObjectWillChange(to parentObjectWillChange: ObservableObjectPublisher) {
        bleService.objectWillChange
            .receive(on: RunLoop.main)
            .sink { [weak parentObjectWillChange] in
                parentObjectWillChange?.send()
            }
            .store(in: &cancellables)
    }

    private func bind() {
        bleService.$sessionState
            .receive(on: RunLoop.main)
            .sink { [weak self] state in
                guard let self else { return }
                self.onRefreshDiagnostics()

                if state.connectionState == .disconnected {
                    self.deviceManager.stopPhoneGpsForwarding()
                }

                guard case let .rerouteRequest(message)? = state.lastInboundMessage else { return }
                let signature = "\(message.routeIdentifier)-\(message.riderLocation.latitude)-\(message.riderLocation.longitude)-\(message.reason)"
                guard self.lastHandledRerouteSignature != signature else { return }
                self.lastHandledRerouteSignature = signature

                Task { @MainActor in
                    await self.onRerouteRequest(message.riderLocation, message.reason)
                }
            }
            .store(in: &cancellables)
    }
}
