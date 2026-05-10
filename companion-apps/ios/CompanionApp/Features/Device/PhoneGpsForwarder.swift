import Foundation

/// Periodically reads the phone's GPS from CoreLocationService and writes
/// it to the device's phone-GPS BLE characteristic at ~1 Hz while phone
/// GPS mode is active. The firmware auto-detects sample writes and
/// switches to Phone GPS mode; when samples stop (disconnect / toggle off),
/// the firmware auto-falls back to Internal GPS after a timeout window
/// (currently 120 seconds in firmware).
@MainActor
final class PhoneGpsForwarder {
    private let bleClient: any RouteSyncBluetoothClient
    private let locationService: any LocationService
    private var forwardingTask: Task<Void, Never>?
    private var lastObservedLocation: CoordinatePoint?
    private var lastMeaningfulMovementAt: Date?

    /// Distance under which location jitter is treated as "still at the same
    /// spot" for stale-speed decay.
    private let movementThresholdMeters: Double = 3.0

    /// True while the forwarder is actively sending GPS data.
    @Published private(set) var isForwarding = false

    init(bleClient: any RouteSyncBluetoothClient, locationService: any LocationService) {
        self.bleClient = bleClient
        self.locationService = locationService
    }

    /// Start forwarding phone GPS at the given interval.
    func start(interval: TimeInterval = 1.0, staleSpeedAfter: TimeInterval = 5.0) {
        guard !isForwarding else { return }
        isForwarding = true
        lastObservedLocation = nil
        lastMeaningfulMovementAt = nil
        forwardingTask = Task { [weak self] in
            guard let self else { return }
            while !Task.isCancelled {
                let now = Date()
                if let location = self.locationService.currentLocation {
                    self.updateMovementWindow(for: location, now: now)
                    let freshSpeed = self.locationService.currentSpeedMps ?? 0
                    let secondsSinceMovement = now.timeIntervalSince(self.lastMeaningfulMovementAt ?? now)
                    let speed = secondsSinceMovement >= staleSpeedAfter ? 0 : freshSpeed
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

    private func updateMovementWindow(for location: CoordinatePoint, now: Date) {
        guard let previous = lastObservedLocation else {
            lastObservedLocation = location
            lastMeaningfulMovementAt = now
            return
        }
        let movedMeters = distanceMeters(from: previous, to: location)
        if movedMeters >= movementThresholdMeters {
            lastMeaningfulMovementAt = now
        }
        lastObservedLocation = location
    }

    /// Equirectangular approximation, accurate enough for short rider-step
    /// comparisons and much cheaper than full haversine.
    private func distanceMeters(from a: CoordinatePoint, to b: CoordinatePoint) -> Double {
        let metersPerDegLat = 111_320.0
        let avgLatRad = ((a.latitude + b.latitude) * 0.5) * .pi / 180.0
        let metersPerDegLon = metersPerDegLat * cos(avgLatRad)
        let dLatMeters = (b.latitude - a.latitude) * metersPerDegLat
        let dLonMeters = (b.longitude - a.longitude) * metersPerDegLon
        return hypot(dLatMeters, dLonMeters)
    }

    func stop() {
        forwardingTask?.cancel()
        forwardingTask = nil
        isForwarding = false
        lastObservedLocation = nil
        lastMeaningfulMovementAt = nil
    }
}
