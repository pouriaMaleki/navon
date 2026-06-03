import XCTest
@testable import Navon

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
        vm.searchController.updateQuery("cathedral")
        // Let the async search task land.
        try? await Task.sleep(nanoseconds: 50_000_000)
        XCTAssertFalse(vm.searchController.suggestions.isEmpty)
        vm.searchController.updateQuery("")
        XCTAssertTrue(vm.searchController.suggestions.isEmpty)
    }

    func test_updateQuery_urlInputTriggersUrlResolve() {
        let app = AppModel()
        let search = FakePlaceSearch()
        let vm = HomeViewModel(appModel: app, placeSearchService: search)
        vm.searchController.updateQuery("https://www.google.com/maps/@60.16,24.95,15z")
        // isResolvingUrl flips synchronously inside updateQuery.
        XCTAssertTrue(vm.searchController.isResolvingUrl, "url paste must flip isResolvingUrl before resolve completes")
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
        vm.searchController.updateQuery("station")
        try? await Task.sleep(nanoseconds: 50_000_000)
        XCTAssertFalse(vm.searchController.suggestions.isEmpty)
        // Simulate user backspacing each character.
        for prefix in ["statio", "stati", "stat", "sta", "st", "s", ""] {
            vm.searchController.updateQuery(prefix)
        }
        XCTAssertTrue(vm.searchController.suggestions.isEmpty)
    }

    func test_closeSearch_cancelsInFlightUrlResolve() {
        let app = AppModel()
        let search = FakePlaceSearch()
        let vm = HomeViewModel(appModel: app, placeSearchService: search)
        vm.searchController.updateQuery("https://www.google.com/maps/@60.16,24.95,15z")
        XCTAssertTrue(vm.searchController.isResolvingUrl)
        vm.searchController.closeSearch()
        XCTAssertFalse(vm.searchController.isResolvingUrl)
        XCTAssertNil(vm.searchController.urlResolveError)
    }

    func test_loadMoreRecentsIfNeeded_isNoOpForNonLastItem() {
        // Spec lines 71-73: recents pagination is gated on reaching the end
        // of the visible slice. Passing an id that isn't the last visible
        // item must not grow the count.
        let app = AppModel()
        let vm = HomeViewModel(appModel: app)
        let before = vm.searchController.visibleRecentCount
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
        vm.searchController.loadMoreRecentsIfNeeded(for: notLast)
        XCTAssertEqual(
            vm.searchController.visibleRecentCount,
            before,
            "load-more must be gated on the last visible item"
        )
    }

    func test_initialVisibleRecentCountIsBounded() {
        // Spec lines 72-73: the dropdown shows "only a few" initially.
        let app = AppModel()
        let vm = HomeViewModel(appModel: app)
        XCTAssertLessThan(
            vm.searchController.visibleRecentCount,
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
        vm.searchController.openSearch()
        XCTAssertTrue(vm.searchController.isSearchOpen)
        vm.selectSuggestion(search.searchResults[0])
        XCTAssertFalse(
            vm.searchController.isSearchOpen,
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
        vm.searchController.openSearch()
        vm.selectSuggestion(search.searchResults[0])
        XCTAssertFalse(vm.searchController.isSearchOpen)
        // Simulate the SwiftUI re-focus replay.
        vm.searchController.openSearch()
        XCTAssertFalse(
            vm.searchController.isSearchOpen,
            "openSearch within the post-selection latch window must be a no-op (anti-re-focus)"
        )
    }

    func test_openSearch_isLatchedAfterSelectingRecent() {
        // Same latch contract for the recents path. The earlier-discovered
        // bug was that `selectRecent` did NOT arm the latch (only
        // `selectSuggestion` did), so tapping a recent could re-open the
        // dropdown via the SwiftUI binding echo and immediately kick off
        // a fresh search for the address the user just picked.
        let app = AppModel()
        let vm = HomeViewModel(appModel: app)
        let recent = RouteHistoryItem(
            id: "recent-test",
            title: "Kolmas Linja 19",
            subtitle: "Saved",
            source: .recentDestination,
            sourceLabel: "Recent",
            createdAt: Date(),
            destination: CoordinatePoint(latitude: 60.184, longitude: 24.952),
            routePackage: nil,
            occurrenceCount: nil
        )
        vm.searchController.openSearch()
        vm.searchController.selectRecent(recent)
        vm.searchController.closeSearch()
        XCTAssertFalse(vm.searchController.isSearchOpen)
        // Re-focus replay within the latch window.
        vm.searchController.openSearch()
        XCTAssertFalse(
            vm.searchController.isSearchOpen,
            "selectRecent must arm the same post-selection latch as selectSuggestion"
        )
    }
}
