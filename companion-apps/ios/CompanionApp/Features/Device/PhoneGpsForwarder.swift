import Foundation

/// Periodically reads the phone's GPS from CoreLocationService and writes
/// it to the device's phone-GPS BLE characteristic at ~1 Hz while phone
/// GPS mode is active. The firmware auto-detects sample writes and
/// switches to Phone GPS mode; when samples stop (disconnect / toggle off),
/// the firmware auto-falls back to Internal GPS after 3 seconds.
@MainActor
final class PhoneGpsForwarder {
    private let bleClient: any RouteSyncBluetoothClient
    private let locationService: CoreLocationService
    private var forwardingTask: Task<Void, Never>?

    /// True while the forwarder is actively sending GPS data.
    @Published private(set) var isForwarding = false

    init(bleClient: any RouteSyncBluetoothClient, locationService: CoreLocationService) {
        self.bleClient = bleClient
        self.locationService = locationService
    }

    /// Start forwarding phone GPS at the given interval.
    func start(interval: TimeInterval = 1.0) {
        guard !isForwarding else { return }
        isForwarding = true
        forwardingTask = Task { [weak self] in
            guard let self else { return }
            while !Task.isCancelled {
                if let location = self.locationService.currentLocation {
                    let speed = self.locationService.currentSpeedMps ?? 0
                    try? await self.bleClient.writePhoneGpsSample(
                        lat: location.latitude,
                        lon: location.longitude,
                        speed: speed,
                        course: nil,
                        accuracy: nil
                    )
                } else if let lastKnown = self.locationService.lastKnownLocation {
                    // Send last-known location when no current fix is
                    // available (indoor, tunnel) so the device still has
                    // a position to center the map on.
                    try? await self.bleClient.writePhoneGpsSample(
                        lat: lastKnown.latitude,
                        lon: lastKnown.longitude,
                        speed: 0,
                        course: nil,
                        accuracy: nil
                    )
                }
                try? await Task.sleep(nanoseconds: UInt64(interval * 1_000_000_000))
            }
        }
    }

    func stop() {
        forwardingTask?.cancel()
        forwardingTask = nil
        isForwarding = false
    }
}
