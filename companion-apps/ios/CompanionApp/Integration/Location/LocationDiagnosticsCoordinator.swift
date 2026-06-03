import Foundation
import Combine

/// Observes location updates, updates the route request origin, and records
/// throttled location events to routing diagnostics. Extracted from
/// AppModel.bindLocationService().
@MainActor
final class LocationDiagnosticsCoordinator {
    private let locationService: CoreLocationService
    private let routingDiagnosticsStore: RoutingDiagnosticsStore
    private let onLocationUpdate: (CoordinatePoint) -> Void

    private var cancellables = Set<AnyCancellable>()
    private var lastRecordedLocationMs: UInt64 = 0

    init(
        locationService: CoreLocationService,
        routingDiagnosticsStore: RoutingDiagnosticsStore,
        onLocationUpdate: @escaping (CoordinatePoint) -> Void
    ) {
        self.locationService = locationService
        self.routingDiagnosticsStore = routingDiagnosticsStore
        self.onLocationUpdate = onLocationUpdate
        bind()
    }

    /// Forwards `locationService.objectWillChange` to the given publisher so
    /// SwiftUI views observing the parent re-render on location changes.
    func forwardObjectWillChange(to parentObjectWillChange: ObservableObjectPublisher) {
        locationService.objectWillChange
            .receive(on: RunLoop.main)
            .sink { [weak parentObjectWillChange] in
                parentObjectWillChange?.send()
            }
            .store(in: &cancellables)
    }

    private func bind() {
        locationService.$currentLocation
            .receive(on: RunLoop.main)
            .sink { [weak self] point in
                guard let self, let point else { return }
                self.onLocationUpdate(point)

                if self.routingDiagnosticsStore.isRecording {
                    let now = UInt64(Date().timeIntervalSince1970 * 1000)
                    if now - self.lastRecordedLocationMs >= LOCATION_EVENT_THROTTLE_MS {
                        self.lastRecordedLocationMs = now
                        self.routingDiagnosticsStore.recordEvent(.locationUpdate(
                            lat: point.latitude,
                            lon: point.longitude,
                            heading: self.locationService.currentHeadingDegrees,
                            speed: self.locationService.currentSpeedMps,
                            accuracyM: nil
                        ))
                    }
                }
            }
            .store(in: &cancellables)
    }
}
