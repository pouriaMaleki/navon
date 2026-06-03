import XCTest
@testable import Navon

/// L2 URL-paste tests (plan flows #29-31).
@MainActor
final class HomeViewModelUrlPasteTests: XCTestCase {

    func test_inlineCoordUrl_populatesRouteRequest() async {
        let app = AppModel()
        let search = FakePlaceSearch()
        search.resolveResult = DestinationSearchResult(
            id: "resolved",
            title: "Helsinki",
            subtitle: "",
            coordinate: CoordinatePoint(latitude: 60.16, longitude: 24.95)
        )
        let vm = HomeViewModel(appModel: app, placeSearchService: search)
        vm.searchController.updateQuery("https://www.google.com/maps/@60.16,24.95,15z")

        // Wait for the resolve task to complete (up to 1 second — plenty
        // for an inline-coord path that does not hit the network).
        for _ in 0..<40 {
            if !vm.searchController.isResolvingUrl { break }
            try? await Task.sleep(nanoseconds: 25_000_000)
        }
        XCTAssertFalse(vm.searchController.isResolvingUrl)
        XCTAssertNil(vm.searchController.urlResolveError)
        XCTAssertEqual(app.routeRequest.destination.latitude, 60.16, accuracy: 0.001)
        XCTAssertEqual(app.routeRequest.destination.longitude, 24.95, accuracy: 0.001)
    }

    func test_nonUrlQueryDoesNotFlipIsResolvingUrl() async {
        let app = AppModel()
        let search = FakePlaceSearch()
        let vm = HomeViewModel(appModel: app, placeSearchService: search)
        vm.searchController.updateQuery("helsinki central")
        XCTAssertFalse(vm.searchController.isResolvingUrl)
    }

    func test_closeSearchClearsUrlErrorAndFlag() {
        let app = AppModel()
        let search = FakePlaceSearch()
        let vm = HomeViewModel(appModel: app, placeSearchService: search)
        vm.searchController.updateQuery("https://maps.app.goo.gl/test")
        vm.searchController.closeSearch()
        XCTAssertFalse(vm.searchController.isResolvingUrl)
        XCTAssertNil(vm.searchController.urlResolveError)
    }
}
