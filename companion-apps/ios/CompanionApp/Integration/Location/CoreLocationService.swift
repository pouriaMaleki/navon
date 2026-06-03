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
    @Published private(set) var currentHeadingDegrees: Double?
    /// True when the user has asked for "Always" but iOS only granted
    /// "When-In-Use". Spec line 130 — surface a hint pointing the user to
    /// the iOS Settings app to flip the toggle manually.
    @Published private(set) var manualSettingsHint: Bool = false

    static let defaultFallback = CoordinatePoint(latitude: 60.1699, longitude: 24.9384)

    /// Internal access (not `private`) so unit tests in
    /// `CoreLocationBackgroundConfigTests` can read the manager's
    /// background-config flags. CLLocationManager itself is a thin wrapper
    /// over an Apple-managed singleton so exposing the reference is safe.
    let manager: CLLocationManager
    private let persistence: CompanionPersistence
    /// Visible to tests so the background-location gating contract can
    /// be checked without depending on simulator authorization state
    /// (which decides whether `isLocating` flips true at start time).
    private(set) var watching: Bool = false
    private var requestedAlways: Bool = false

    init(persistence: CompanionPersistence) {
        self.manager = CLLocationManager()
        self.persistence = persistence
        super.init()
        manager.delegate = self
        // Planning-mode defaults — adequate for showing the rider dot on the
        // map without hammering the GPS radio. Tightened to navigation-grade
        // via setNavigationAccuracy(true) once a route starts.
        manager.desiredAccuracy = kCLLocationAccuracyBest
        manager.distanceFilter = 10
        // Background-readiness configuration. Without these flags iOS
        // silently pauses GPS once its motion heuristics decide the
        // rider has stopped — fatal for cycling, where coasting/headwind
        // looks identical to a stationary phone. See
        // CoreLocationBackgroundConfigTests for the contract.
        manager.allowsBackgroundLocationUpdates = true
        manager.pausesLocationUpdatesAutomatically = false
        manager.activityType = .otherNavigation
        if let stored = persistence.loadLastKnownRider() {
            lastKnownLocation = stored
        }
    }

    var bestLocation: CoordinatePoint {
        currentLocation ?? lastKnownLocation ?? Self.defaultFallback
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

    /// Switches the CLLocationManager between navigation-grade and planning-mode
    /// accuracy. Navigation mode uses `kCLLocationAccuracyBestForNavigation`
    /// (keeps the GPS radio fully awake) and no distance filter so every fix
    /// is delivered — mirroring how OsmAnd and OwnTracks keep background GPS
    /// reliable. Planning mode backs off to `kCLLocationAccuracyBest` + a 10 m
    /// filter, which is sufficient for showing the rider dot on the map.
    func setNavigationAccuracy(_ active: Bool) {
        if active {
            manager.desiredAccuracy = kCLLocationAccuracyBestForNavigation
            manager.distanceFilter = kCLDistanceFilterNone
        } else {
            manager.desiredAccuracy = kCLLocationAccuracyBest
            manager.distanceFilter = 10
        }
    }

    /// Request "Always" authorization for background GPS. iOS only allows
    /// this prompt to escalate from "When-In-Use" once; subsequent attempts
    /// silently no-op, in which case the user must flip the toggle in
    /// Settings (we surface that via `manualSettingsHint`).
    func requestAlwaysAuthorization() {
        requestedAlways = true
        switch manager.authorizationStatus {
        case .authorizedAlways:
            manualSettingsHint = false
        case .authorizedWhenInUse:
            manager.requestAlwaysAuthorization()
        case .notDetermined:
            manager.requestWhenInUseAuthorization()
        case .denied, .restricted:
            manualSettingsHint = true
        @unknown default:
            break
        }
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
        let heading: Double? = location.course >= 0 && location.course.isFinite ? location.course : nil
        Task { @MainActor in
            self.handleFix(point, speedMps: speed, headingDegrees: heading)
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
        case .authorizedWhenInUse:
            if watching { manager.startUpdatingLocation() }
            // We asked for Always but iOS held us at WhenInUse.
            manualSettingsHint = requestedAlways
        case .authorizedAlways:
            if watching { manager.startUpdatingLocation() }
            manager.allowsBackgroundLocationUpdates = true
            manualSettingsHint = false
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
    private func handleFix(_ point: CoordinatePoint, speedMps: Double?, headingDegrees: Double?) {
        currentLocation = point
        lastKnownLocation = point
        currentSpeedMps = speedMps
        currentHeadingDegrees = headingDegrees
        isLocating = false
        lastError = nil
        persistence.saveLastKnownRider(point)
    }
}
