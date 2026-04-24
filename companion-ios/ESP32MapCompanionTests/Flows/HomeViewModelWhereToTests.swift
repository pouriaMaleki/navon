import XCTest
@testable import ESP32MapCompanion

/// L2 tests for the where-to dropdown surface (plan flows #20, #25, #35-37).
/// Drives `HomeViewModel` with a FakePlaceSearch and asserts the observable
/// state it exposes.
@MainActor
final class HomeViewModelWhereToTests: XCTestCase {

    func test_updateQuery_blankClearsSuggestions() async {
        let app = AppModel()
        let search = FakePlaceSearch()
        let vm = HomeViewModel(appModel: app, placeSearchService: search)
        search.searchResults = [
            DestinationSearchResult(
                id: "r1",
                title: "Cathedral",
                subtitle: "",
                coordinate: CoordinatePoint(latitude: 60.17, longitude: 24.95)
            )
        ]
        vm.updateQuery("cathedral")
        // Let the async search task land.
        try? await Task.sleep(nanoseconds: 50_000_000)
        XCTAssertFalse(vm.suggestions.isEmpty)
        vm.updateQuery("")
        XCTAssertTrue(vm.suggestions.isEmpty)
    }

    func test_updateQuery_urlInputTriggersUrlResolve() {
        let app = AppModel()
        let search = FakePlaceSearch()
        let vm = HomeViewModel(appModel: app, placeSearchService: search)
        vm.updateQuery("https://www.google.com/maps/@60.16,24.95,15z")
        // isResolvingUrl flips synchronously inside updateQuery.
        XCTAssertTrue(vm.isResolvingUrl, "url paste must flip isResolvingUrl before resolve completes")
    }

    func test_manualKeyboardDeleteClearsSuggestions() async {
        let app = AppModel()
        let search = FakePlaceSearch()
        let vm = HomeViewModel(appModel: app, placeSearchService: search)
        search.searchResults = [
            DestinationSearchResult(
                id: "r1",
                title: "Station",
                subtitle: "",
                coordinate: CoordinatePoint(latitude: 60.17, longitude: 24.95)
            )
        ]
        vm.updateQuery("station")
        try? await Task.sleep(nanoseconds: 50_000_000)
        XCTAssertFalse(vm.suggestions.isEmpty)
        // Simulate user backspacing each character.
        for prefix in ["statio", "stati", "stat", "sta", "st", "s", ""] {
            vm.updateQuery(prefix)
        }
        XCTAssertTrue(vm.suggestions.isEmpty)
    }

    func test_closeSearch_cancelsInFlightUrlResolve() {
        let app = AppModel()
        let search = FakePlaceSearch()
        let vm = HomeViewModel(appModel: app, placeSearchService: search)
        vm.updateQuery("https://www.google.com/maps/@60.16,24.95,15z")
        XCTAssertTrue(vm.isResolvingUrl)
        vm.closeSearch()
        XCTAssertFalse(vm.isResolvingUrl)
        XCTAssertNil(vm.urlResolveError)
    }

    func test_loadMoreRecentsIfNeeded_isNoOpForNonLastItem() {
        // Spec lines 71-73: recents pagination is gated on reaching the end
        // of the visible slice. Passing an id that isn't the last visible
        // item must not grow the count.
        let app = AppModel()
        let vm = HomeViewModel(appModel: app)
        let before = vm.visibleRecentCount
        let notLast = RouteHistoryItem(
            id: "not-the-last-id",
            title: "Test",
            subtitle: "",
            source: .recentDestination,
            sourceLabel: "Recent",
            createdAt: Date(),
            destination: CoordinatePoint(latitude: 60.17, longitude: 24.95),
            routePackage: nil,
            occurrenceCount: nil
        )
        vm.loadMoreRecentsIfNeeded(for: notLast)
        XCTAssertEqual(
            vm.visibleRecentCount,
            before,
            "load-more must be gated on the last visible item"
        )
    }

    func test_initialVisibleRecentCountIsBounded() {
        // Spec lines 72-73: the dropdown shows "only a few" initially.
        let app = AppModel()
        let vm = HomeViewModel(appModel: app)
        XCTAssertLessThan(
            vm.visibleRecentCount,
            30,
            "initial recents slice must be bounded so large histories don't render everything up-front"
        )
    }

    // MARK: - Search-panel dismissal on suggestion select (regression for
    // "dropdown stays open after pick on real device")

    func test_selectSuggestion_closesDropdownImmediately() async {
        let app = AppModel()
        let search = FakePlaceSearch()
        let vm = HomeViewModel(appModel: app, placeSearchService: search)
        search.searchResults = [
            DestinationSearchResult(
                id: "r1",
                title: "Cathedral",
                subtitle: "",
                coordinate: CoordinatePoint(latitude: 60.17, longitude: 24.95)
            )
        ]
        vm.openSearch()
        XCTAssertTrue(vm.isSearchOpen)
        vm.selectSuggestion(search.searchResults[0])
        XCTAssertFalse(
            vm.isSearchOpen,
            "selectSuggestion must close the dropdown synchronously"
        )
    }

    func test_openSearch_isLatchedAfterSelection() {
        // The real-device bug is the SwiftUI TextField re-fires its focus
        // tracker after selection, replaying openSearch(). The latch must
        // absorb that follow-up open while still allowing later opens
        // (after the latch window) to work normally.
        let app = AppModel()
        let search = FakePlaceSearch()
        let vm = HomeViewModel(appModel: app, placeSearchService: search)
        search.searchResults = [
            DestinationSearchResult(
                id: "r1",
                title: "Kallio",
                subtitle: "",
                coordinate: CoordinatePoint(latitude: 60.184, longitude: 24.952)
            )
        ]
        vm.openSearch()
        vm.selectSuggestion(search.searchResults[0])
        XCTAssertFalse(vm.isSearchOpen)
        // Simulate the SwiftUI re-focus replay.
        vm.openSearch()
        XCTAssertFalse(
            vm.isSearchOpen,
            "openSearch within the post-selection latch window must be a no-op (anti-re-focus)"
        )
    }
}
