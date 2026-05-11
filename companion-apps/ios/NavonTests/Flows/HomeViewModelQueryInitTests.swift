import XCTest
@testable import Navon

@MainActor
final class HomeViewModelQueryInitTests: XCTestCase {

    private func freshDefaults() -> UserDefaults {
        UserDefaults(suiteName: "home-vm-query-init-\(UUID().uuidString)")!
    }

    private func makeViewModel(
        sessionDestinationLabel: String,
        defaults: UserDefaults? = nil
    ) -> HomeViewModel {
        let defaults = defaults ?? freshDefaults()
        // Pre-seed a persisted session so AppModel.init loads it. The shape
        // mirrors what providers actually persist when the route had no
        // destination label — in that case they fall through to the
        // "Selected destination" placeholder.
        let session = ActiveRouteSession(
            routeIdentifier: "stale-route-id",
            routeRevision: 1,
            destinationLabel: sessionDestinationLabel,
            destinationCoordinate: CoordinatePoint(latitude: 60.17, longitude: 24.94),
            providerID: .osm,
            sourceMode: .osm,
            lastRerouteReason: nil,
            lastRerouteTimestamp: nil
        )
        let persistence = CompanionPersistence(defaults: defaults)
        persistence.saveSession(session)

        let appModel = AppModel(persistence: persistence)
        return HomeViewModel(appModel: appModel, placeSearchService: FakePlaceSearch())
    }

    func test_syncQuery_skipsSelectedDestinationPlaceholder() {
        let vm = makeViewModel(sessionDestinationLabel: "Selected destination")
        vm.syncQueryFromCurrentPreview()
        XCTAssertEqual(vm.query, "", "Generic 'Selected destination' placeholder must not prefill the search field")
    }

    func test_syncQuery_skipsAllGenericPlaceholders() {
        for placeholder in ["No destination", "Selected destination", "Recent destination", "Dropped pin", "Route", ""] {
            let vm = makeViewModel(sessionDestinationLabel: placeholder)
            vm.syncQueryFromCurrentPreview()
            XCTAssertEqual(vm.query, "", "Placeholder '\(placeholder)' should not prefill the search field")
        }
    }

    func test_syncQuery_preservesRealDestinationLabel() {
        let vm = makeViewModel(sessionDestinationLabel: "Helsinki Central Station")
        vm.syncQueryFromCurrentPreview()
        XCTAssertEqual(vm.query, "Helsinki Central Station")
    }
}
