import XCTest
@testable import Navon

/// User-reported bug: on iOS, rerouting "appears to work" — the rider
/// hears the rerouting cue and sees the rerouting banner — but no
/// new route is actually fetched. The audio plays and then guidance
/// stays stuck on the stale path.
///
/// Root cause: `HomeViewModel` flips `rerouteRequested = true` after
/// the rider dwells off-route past `rerouteRequestDelayMs`, but no
/// observer turns that flag into a call to
/// `AppModel.rerouteActiveSession(...)`. The web app already has this
/// wiring via a MobX `when()` reaction; iOS needs the equivalent.
///
/// This file pins the behaviour: when the rider drifts off-route long
/// enough for the request signal to fire, AppModel must record an
/// auto-reroute attempt.
@MainActor
final class AutoRerouteTests: XCTestCase {

    private func offset(_ base: CoordinatePoint, eastM: Double, northM: Double) -> CoordinatePoint {
        let metersPerDegLat = 111_320.0
        let meanLat = base.latitude * .pi / 180.0
        return CoordinatePoint(
            latitude: base.latitude + northM / metersPerDegLat,
            longitude: base.longitude + eastM / (metersPerDegLat * cos(meanLat))
        )
    }

    /// Straight 800 m route heading north — easy to drift sideways
    /// for a clean perpendicular off-route condition.
    private func straightRoute() -> NormalizedRoutePackage {
        let metersPerDegLat = 111_320.0
        let start = CoordinatePoint(latitude: 60.17, longitude: 24.94)
        let end = CoordinatePoint(
            latitude: 60.17 + 800.0 / metersPerDegLat,
            longitude: 24.94
        )
        return NormalizedRoutePackage(
            version: RoutePackageVersion.current,
            routeIdentifier: "straight",
            revision: 1,
            geometry: [start, end],
            maneuvers: [
                RouteManeuver(id: "m1", maneuverType: .depart, location: start,
                              distanceFromStartMeters: 0, distanceToNextMeters: 800, instructionText: nil),
                RouteManeuver(id: "m2", maneuverType: .arrive, location: end,
                              distanceFromStartMeters: 800, distanceToNextMeters: nil, instructionText: nil),
            ],
            summary: RouteSummary(totalDistanceMeters: 800, estimatedDurationSeconds: 240,
                                  startLabel: nil, destinationLabel: nil),
            provenance: RouteProvenance(providerID: .osm, sourceReference: nil, generatedAtUnixMs: 0)
        )
    }

    private func startRoute(_ pkg: NormalizedRoutePackage) async -> (AppModel, HomeViewModel) {
        let app = AppModel()
        let vm = HomeViewModel(appModel: app)
        app.preview = RoutePreviewModel(
            alternatives: [RouteAlternative(
                id: UUID(), title: "Straight", subtitle: "",
                distanceMeters: 800, durationSeconds: 240, normalizedPackage: pkg
            )],
            selectedAlternativeID: nil, routeIdentifier: nil, routeRevision: nil, planningNotice: nil
        )
        // The auto-reroute path uses `activeSession.destinationCoordinate`
        // to know where to plan toward; populate it as `startSelectedRoute`
        // would, plus stash the destination on the routeRequest so the
        // test doesn't depend on a real destination search.
        await vm.startSelectedRoute()
        app.activeSession.destinationCoordinate = pkg.geometry.last
        return (app, vm)
    }

    func test_offRouteDwellTriggersAutoReroute() async {
        let pkg = straightRoute()
        let (app, vm) = await startRoute(pkg)

        let start = pkg.geometry[0]
        let drifted = offset(start, eastM: 50, northM: 0)
        // Spec: 35 m enter / 22 m exit hysteresis, then 2000 ms dwell
        // before `rerouteRequested` flips. Tick from t=0 to t=3000 ms
        // with a constant 50 m off-route position.
        vm.ingestRiderLocationFix(drifted, timestampMs: 0)
        vm.ingestRiderLocationFix(drifted, timestampMs: 1500)
        vm.ingestRiderLocationFix(drifted, timestampMs: 3000)

        // Auto-reroute is dispatched as a Task — wait for it.
        await vm.pendingAutoRerouteTask?.value

        XCTAssertNotNil(
            app.activeSession.lastRerouteReason,
            "off-route dwell should have triggered AppModel.rerouteActiveSession, but lastRerouteReason was nil — only the audio cue plays today"
        )
        XCTAssertNotNil(
            app.activeSession.lastRerouteTimestamp,
            "auto-reroute must record a timestamp so subsequent off-route episodes don't double-fire"
        )
    }

    func test_stopNavigation_clearsAllRoutingState() async {
        // After stopActiveNavigation (manual stop OR arrival), every piece of
        // routing state must be zeroed so the next ride starts clean and no
        // stale @Published values linger in the UI.
        let pkg = straightRoute()
        let (app, vm) = await startRoute(pkg)

        // Build up some mid-route state: advance the rider, trigger off-route.
        // The route runs pure north, so a fix 50 m due east of the START
        // projects onto t=0 and progress stays 0 (perpendicular to the
        // segment). Use a fix that's both 100 m along the route AND 50 m
        // east so projection advances and the off-route latch trips.
        let start = pkg.geometry[0]
        let onRoute = offset(start, eastM: 0, northM: 100) // 100 m along route
        let drifted = offset(start, eastM: 50, northM: 100) // 50 m east of the 100 m mark
        vm.ingestRiderLocationFix(onRoute, timestampMs: 0)
        vm.ingestRiderLocationFix(drifted, timestampMs: 1_000)
        // offRoute should be latched
        XCTAssertTrue(vm.offRoute)
        XCTAssertGreaterThan(vm.progressDistanceM, 0)

        vm.stopActiveNavigation()
        // Give the Task a chance to run.
        await Task.yield()

        XCTAssertFalse(app.isRoutingInProgress, "isRoutingInProgress must be false")
        XCTAssertNil(app.activeSession.routeIdentifier, "routeIdentifier must be cleared")
        XCTAssertFalse(vm.offRoute, "offRoute must be reset")
        XCTAssertEqual(vm.offRouteDistanceM, 0, "offRouteDistanceM must be reset")
        XCTAssertFalse(vm.rerouteRequested, "rerouteRequested must be reset")
        XCTAssertEqual(vm.progressDistanceM, 0, "progressDistanceM must be reset")
        XCTAssertNil(vm.arrivalNotice, "arrivalNotice must be cleared on stop")
        XCTAssertNil(vm.pendingAutoRerouteTask, "in-flight reroute task must be cancelled")
    }

    func test_inFlightReroute_cancelledOnStop() async {
        // If the rider goes off-route and immediately presses Stop before the
        // reroute network call returns, the task must be cancelled and must
        // not write back to activeSession after stop.
        let pkg = straightRoute()
        let (app, vm) = await startRoute(pkg)

        // Trigger auto-reroute (dwell past threshold)
        let drifted = offset(pkg.geometry[0], eastM: 50, northM: 0)
        vm.ingestRiderLocationFix(drifted, timestampMs: 0)
        vm.ingestRiderLocationFix(drifted, timestampMs: 3_000)
        XCTAssertNotNil(vm.pendingAutoRerouteTask, "task should be pending before stop")

        // Stop immediately without awaiting the reroute.
        vm.stopActiveNavigation()
        await Task.yield()

        XCTAssertNil(vm.pendingAutoRerouteTask, "task must be nil after stop")
        XCTAssertFalse(app.isRoutingInProgress)
    }

    func test_sustainedOffRoute_canTriggerFollowUpAutoRerouteWithoutReturningOnRoute() async {
        // Regression guard: after dispatching an auto-reroute, we must
        // re-arm the reroute latch even if the rider is still off-route.
        // Audio cues may be silenced by CueEngine policy, but rerouting
        // itself must continue.
        let pkg = straightRoute()
        let (app, vm) = await startRoute(pkg)

        let drifted = offset(pkg.geometry[0], eastM: 50, northM: 0)
        vm.ingestRiderLocationFix(drifted, timestampMs: 0)
        vm.ingestRiderLocationFix(drifted, timestampMs: 3000)
        await vm.pendingAutoRerouteTask?.value
        let firstStamp = app.activeSession.lastRerouteTimestamp
        XCTAssertNotNil(firstStamp)

        // Keep riding off-route. Another sustained dwell should trigger
        // a follow-up reroute without needing an on-route reset.
        vm.ingestRiderLocationFix(drifted, timestampMs: 4500)
        vm.ingestRiderLocationFix(drifted, timestampMs: 6000)
        await vm.pendingAutoRerouteTask?.value
        XCTAssertNotEqual(
            app.activeSession.lastRerouteTimestamp, firstStamp,
            "sustained off-route after a reroute must still permit another reroute attempt"
        )
    }
}
