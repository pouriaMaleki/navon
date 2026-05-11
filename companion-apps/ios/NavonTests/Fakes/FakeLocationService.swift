import Foundation
@testable import Navon

/// Test double for `LocationService`. Properties are `@Published` and the class
/// conforms to `ObservableObject` so SwiftUI / Combine subscribers see the same
/// change stream they would from the real `CoreLocationService`.
@MainActor
final class FakeLocationService: ObservableObject, LocationService {
    @Published private(set) var currentLocation: CoordinatePoint?
    @Published private(set) var lastKnownLocation: CoordinatePoint?
    @Published private(set) var isLocating: Bool = false
    @Published private(set) var lastError: LocationErrorKind?
    @Published private(set) var currentSpeedMps: Double?

    private(set) var startCount = 0
    private(set) var stopCount = 0

    func start() {
        startCount += 1
        isLocating = true
        lastError = nil
    }

    func stop() {
        stopCount += 1
        isLocating = false
    }

    func setNavigationAccuracy(_ active: Bool) {}

    func emitFix(latitude: Double, longitude: Double, speedMps: Double? = nil) {
        let point = CoordinatePoint(latitude: latitude, longitude: longitude)
        currentLocation = point
        lastKnownLocation = point
        currentSpeedMps = speedMps
        isLocating = false
        lastError = nil
    }

    func emitError(_ kind: LocationErrorKind) {
        lastError = kind
        isLocating = false
    }
}
