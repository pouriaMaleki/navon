import Foundation
import CoreLocation

@MainActor
final class CoreLocationService: NSObject, ObservableObject, LocationService {
    @Published private(set) var currentLocation: CoordinatePoint?
    @Published private(set) var lastKnownLocation: CoordinatePoint?
    @Published private(set) var isLocating: Bool = false
    @Published private(set) var lastError: LocationErrorKind?
    /// Instantaneous ground speed (m/s) from the most recent CLLocation fix.
    /// Negative `CLLocation.speed` means "unavailable" — we coerce that to nil.
    @Published private(set) var currentSpeedMps: Double?

    private let manager: CLLocationManager
    private let persistence: CompanionPersistence
    private var watching: Bool = false

    init(persistence: CompanionPersistence) {
        self.manager = CLLocationManager()
        self.persistence = persistence
        super.init()
        manager.delegate = self
        manager.desiredAccuracy = kCLLocationAccuracyHundredMeters
        manager.distanceFilter = 10
        if let stored = persistence.loadLastKnownRider() {
            lastKnownLocation = stored
        }
    }

    func start() {
        if watching { return }
        watching = true
        isLocating = true
        lastError = nil
        switch manager.authorizationStatus {
        case .notDetermined:
            manager.requestWhenInUseAuthorization()
        case .authorizedWhenInUse, .authorizedAlways:
            manager.startUpdatingLocation()
        case .denied, .restricted:
            isLocating = false
            lastError = .denied
        @unknown default:
            isLocating = false
            lastError = .unavailable
        }
    }

    func stop() {
        if !watching { return }
        watching = false
        manager.stopUpdatingLocation()
        isLocating = false
    }
}

extension CoreLocationService: CLLocationManagerDelegate {
    nonisolated func locationManager(_ manager: CLLocationManager, didChangeAuthorization status: CLAuthorizationStatus) {
        Task { @MainActor in
            self.handleAuthorizationChange(status)
        }
    }

    nonisolated func locationManagerDidChangeAuthorization(_ manager: CLLocationManager) {
        let status = manager.authorizationStatus
        Task { @MainActor in
            self.handleAuthorizationChange(status)
        }
    }

    nonisolated func locationManager(_ manager: CLLocationManager, didUpdateLocations locations: [CLLocation]) {
        guard let location = locations.last else { return }
        let point = CoordinatePoint(latitude: location.coordinate.latitude, longitude: location.coordinate.longitude)
        let speed: Double? = location.speed >= 0 && location.speed.isFinite ? location.speed : nil
        Task { @MainActor in
            self.handleFix(point, speedMps: speed)
        }
    }

    nonisolated func locationManager(_ manager: CLLocationManager, didFailWithError error: Error) {
        let kind = (error as? CLError)?.code == .denied ? LocationErrorKind.denied : .unavailable
        Task { @MainActor in
            self.isLocating = false
            self.lastError = kind
        }
    }

    @MainActor
    private func handleAuthorizationChange(_ status: CLAuthorizationStatus) {
        switch status {
        case .authorizedWhenInUse, .authorizedAlways:
            if watching { manager.startUpdatingLocation() }
        case .denied, .restricted:
            isLocating = false
            lastError = .denied
            manager.stopUpdatingLocation()
        case .notDetermined:
            break
        @unknown default:
            break
        }
    }

    @MainActor
    private func handleFix(_ point: CoordinatePoint, speedMps: Double?) {
        currentLocation = point
        lastKnownLocation = point
        currentSpeedMps = speedMps
        isLocating = false
        lastError = nil
        persistence.saveLastKnownRider(point)
    }
}
