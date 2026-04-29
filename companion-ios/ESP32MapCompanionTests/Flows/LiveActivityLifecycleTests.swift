import XCTest
@testable import ESP32MapCompanion

/// User-reported bug: the lock-screen Live Activity stopped showing during
/// rides, even though the OS still reported the permission as granted.
/// Two root causes both surface here:
///   1. The activity was only started/updated when the user toggled a
///      setting (the only path that called `syncRoutingActivityServices`).
///      A route start did not start the activity.
///   2. `onGuidanceTick` ignored Live Activity entirely, so even after the
///      activity was started it never received per-tick updates and iOS
///      eventually dismissed it for staleness.
///
/// These tests pin both behaviours.
@MainActor
final class LiveActivityLifecycleTests: XCTestCase {

    private func offset(_ base: CoordinatePoint, eastM: Double, northM: Double) -> CoordinatePoint {
        let metersPerDegLat = 111_320.0
        let meanLat = base.latitude * .pi / 180.0
        return CoordinatePoint(
            latitude: base.latitude + northM / metersPerDegLat,
            longitude: base.longitude + eastM / (metersPerDegLat * cos(meanLat))
        )
    }

    private func tinyRoute() -> NormalizedRoutePackage {
        let metersPerDegLat = 111_320.0
        let start = CoordinatePoint(latitude: 60.17, longitude: 24.94)
        let mid = CoordinatePoint(
            latitude: 60.17 + 400.0 / metersPerDegLat,
            longitude: 24.94
        )
        return NormalizedRoutePackage(
            version: RoutePackageVersion.current,
            routeIdentifier: "live-activity-test",
            revision: 1,
            geometry: [start, mid],
            maneuvers: [
                RouteManeuver(id: "depart", maneuverType: .depart, location: start,
                              distanceFromStartMeters: 0, distanceToNextMeters: 400, instructionText: nil),
                RouteManeuver(id: "arrive", maneuverType: .arrive, location: mid,
                              distanceFromStartMeters: 400, distanceToNextMeters: nil, instructionText: nil),
            ],
            summary: RouteSummary(totalDistanceMeters: 400, estimatedDurationSeconds: 120,
                                  startLabel: nil, destinationLabel: "Park"),
            provenance: RouteProvenance(providerID: .osm, sourceReference: nil, generatedAtUnixMs: 0)
        )
    }

    private func makeApp(liveActivity: LiveActivityPort) -> AppModel {
        let app = AppModel()
        app.replaceRoutingActivityCoordinatorForTesting(
            speech: AudioCueDispatchTests.SpeechSpy(),
            liveActivity: liveActivity
        )
        var s = app.settings
        s.allowBackgroundGps = true
        s.liveActivityEnabled = true
        app.settings = s
        return app
    }

    func test_liveActivity_startsOnFirstGuidanceTick_withoutAnySettingsToggle() async {
        let live = SpyLiveActivityPort()
        let app = makeApp(liveActivity: live)
        let vm = HomeViewModel(appModel: app)
        let pkg = tinyRoute()
        app.preview = RoutePreviewModel(
            alternatives: [RouteAlternative(
                id: UUID(), title: "T", subtitle: "",
                distanceMeters: 400, durationSeconds: 120, normalizedPackage: pkg
            )],
            selectedAlternativeID: nil, routeIdentifier: nil, routeRevision: nil, planningNotice: nil
        )
        await vm.startSelectedRoute()
        // First guidance tick — this is the only thing that fires when the
        // user actually rides, so it must start the Live Activity.
        vm.ingestRiderLocationFix(pkg.geometry[0], timestampMs: 0)
        XCTAssertNotNil(
            live.startedWith,
            "Live Activity must start on the first guidance tick of an active route — got nothing"
        )
    }

    func test_liveActivity_isClearedOnAppLaunch_toCoverForceQuitWhileRouting() {
        // User-reported regression: force-swiping the app mid-route left
        // a phantom Live Activity on the lock screen. Tapping it reopened
        // the app to a clean planning screen but the activity stuck
        // around. ActivityKit deliberately keeps activities alive across
        // app deaths (delivery-tracker use case); we override that on
        // every cold launch — `isRoutingInProgress` is false at launch
        // by definition, so any leftover activity is stale.
        let live = SpyLiveActivityPort()
        // The order matters: AppModel is the constructor we want to
        // observe, so build it AFTER instantiating the spy. The
        // `replaceRoutingActivityCoordinatorForTesting` seam swaps the
        // coordinator BUT not the original port that init() called
        // `endAllOutstanding()` on; we want to assert against the actual
        // production init path, so pass a fake-injection AppModel.
        let app = AppModel.makeForTestingWithLiveActivityPort(live)
        XCTAssertGreaterThanOrEqual(
            live.endAllOutstandingCount, 1,
            "AppModel.init must invoke endAllOutstanding() so a force-quit ride doesn't leave a phantom Live Activity on the lock screen — got \(live.endAllOutstandingCount)"
        )
        _ = app
    }

    func test_liveActivity_doesNotStart_whenNoRouteIsInProgress() {
        // User-reported regression: opening the app without ever pressing
        // Start was triggering the lock-screen activity. Root cause was
        // that `syncRoutingActivityServices` used a stale
        // `activeSession.routeIdentifier` (loaded from disk) as the
        // "is the rider currently riding" signal. The fix is an explicit
        // `AppModel.isRoutingInProgress` flag that only flips to true
        // when the user actually presses Start, and a cold launch leaves
        // it false even if there's a stored routeIdentifier.
        let live = SpyLiveActivityPort()
        let app = makeApp(liveActivity: live)
        // Even with a stale routeIdentifier hanging around (simulating a
        // crashed prior run), the explicit flag is false so no activity
        // should start.
        var stale = app.activeSession
        stale.routeIdentifier = "ghost-route"
        stale.destinationLabel = "Old destination"
        app.activeSession = stale
        XCTAssertFalse(app.isRoutingInProgress)
        app.syncRoutingActivityServices()
        XCTAssertNil(
            live.startedWith,
            "Live Activity must not start when no route is actively in progress — got \(String(describing: live.startedWith))"
        )
    }

    func test_liveActivity_isEndedWhenStopActiveNavigationRuns() async {
        // After a route ends, the activity should be torn down. Without
        // the fix, the routeIdentifier remained set and the next settings
        // toggle re-started the activity.
        let live = SpyLiveActivityPort()
        let app = makeApp(liveActivity: live)
        let vm = HomeViewModel(appModel: app)
        let pkg = tinyRoute()
        app.preview = RoutePreviewModel(
            alternatives: [RouteAlternative(
                id: UUID(), title: "T", subtitle: "",
                distanceMeters: 400, durationSeconds: 120, normalizedPackage: pkg
            )],
            selectedAlternativeID: nil, routeIdentifier: nil, routeRevision: nil, planningNotice: nil
        )
        await vm.startSelectedRoute()
        vm.ingestRiderLocationFix(pkg.geometry[0], timestampMs: 0)
        XCTAssertNotNil(live.startedWith, "Sanity: activity should be running")

        vm.stopActiveNavigation()
        // stopActiveNavigation kicks an async Task; let it land.
        try? await Task.sleep(nanoseconds: 100_000_000)

        XCTAssertGreaterThanOrEqual(
            live.endedCount, 1,
            "Stopping the route must end the Live Activity — got \(live.endedCount) end() calls"
        )
        XCTAssertNil(
            app.activeSession.routeIdentifier,
            "After stopping, activeSession.routeIdentifier must be cleared so a later settings toggle does not restart the activity from a stale id"
        )
    }

    func test_liveActivity_updatesOnEachGuidanceTick() async {
        let live = SpyLiveActivityPort()
        let app = makeApp(liveActivity: live)
        let vm = HomeViewModel(appModel: app)
        let pkg = tinyRoute()
        app.preview = RoutePreviewModel(
            alternatives: [RouteAlternative(
                id: UUID(), title: "T", subtitle: "",
                distanceMeters: 400, durationSeconds: 120, normalizedPackage: pkg
            )],
            selectedAlternativeID: nil, routeIdentifier: nil, routeRevision: nil, planningNotice: nil
        )
        await vm.startSelectedRoute()
        vm.ingestRiderLocationFix(pkg.geometry[0], timestampMs: 0)
        vm.ingestRiderLocationFix(offset(pkg.geometry[0], eastM: 0, northM: 50), timestampMs: 1_000)
        vm.ingestRiderLocationFix(offset(pkg.geometry[0], eastM: 0, northM: 100), timestampMs: 2_000)
        XCTAssertGreaterThanOrEqual(
            live.updates.count, 1,
            "Live Activity must receive update() calls on subsequent guidance ticks so iOS doesn't dismiss it for staleness — got \(live.updates.count) updates"
        )
    }
}
