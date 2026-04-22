import XCTest
@testable import ESP32MapCompanion

/// L2 camera / compass state-machine tests (plan flows #44, #45, #52).
@MainActor
final class CameraModeTests: XCTestCase {

    func test_handleCompassTap_outsideGuidance_isNoOp() {
        let app = AppModel()
        let vm = HomeViewModel(appModel: app)
        vm.handleCompassTap()
        XCTAssertEqual(vm.compassMode, .autoFollow)
    }

    func test_compassDoubleTapLocks_thenTapReturnsToAutoFollow() async {
        let app = AppModel()
        let vm = HomeViewModel(appModel: app)
        // Move into guidance by seeding a preview + starting.
        let pkg = NormalizedRoutePackage(
            version: RoutePackageVersion.current,
            routeIdentifier: "t",
            revision: 1,
            geometry: [
                CoordinatePoint(latitude: 60.17, longitude: 24.94),
                CoordinatePoint(latitude: 60.18, longitude: 24.95),
            ],
            maneuvers: [],
            summary: RouteSummary(
                totalDistanceMeters: 100,
                estimatedDurationSeconds: 60,
                startLabel: nil,
                destinationLabel: nil
            ),
            provenance: RouteProvenance(providerID: .osm, sourceReference: nil, generatedAtUnixMs: 0)
        )
        app.preview = RoutePreviewModel(
            alternatives: [RouteAlternative(
                id: UUID(), title: "R", subtitle: "",
                distanceMeters: 100, durationSeconds: 60, normalizedPackage: pkg
            )],
            selectedAlternativeID: nil, routeIdentifier: nil, routeRevision: nil, planningNotice: nil
        )
        await vm.startSelectedRoute()
        vm.handleCompassDoubleTap()
        XCTAssertEqual(vm.compassMode, .northLocked)
        vm.handleCompassTap()
        XCTAssertEqual(vm.compassMode, .autoFollow)
    }

    func test_companion_north_indicator_single_tap_also_recenters() async {
        // Plan flow #52. Spec line 39: on companion apps the north indicator
        // also recenters the camera. `handleCompassTap` must bump
        // `mapRecenterRequestID` so the map view observes the recenter
        // request.
        let app = AppModel()
        let vm = HomeViewModel(appModel: app)

        // Must be in guidance for the spec flow to apply. Without that
        // precondition, handleCompassTap is a no-op (the test that covers
        // that case is `test_handleCompassTap_outsideGuidance_isNoOp`).
        vm.homeMode = .phoneGuidance
        let before = vm.mapRecenterRequestID
        vm.handleCompassTap()
        XCTAssertNotEqual(
            vm.mapRecenterRequestID,
            before,
            "handleCompassTap must bump mapRecenterRequestID on every tap"
        )
    }

    // MARK: - Follow rider during routing (spec line 84)

    func test_followRider_duringRouting_bumpsFollowTick() {
        // Spec line 84: "camera moves so that user location is on the bottom
        // quarter of the screen". For the camera to keep tracking the rider
        // as they move, HomeViewModel must expose a `mapFollowRiderTick`
        // that bumps on every GPS update while in phoneGuidance.
        let app = AppModel()
        let vm = HomeViewModel(appModel: app)
        vm.homeMode = .phoneGuidance
        let before = vm.mapFollowRiderTick
        vm.notifyRiderLocationUpdated()
        XCTAssertNotEqual(
            vm.mapFollowRiderTick,
            before,
            "A rider-location update during phoneGuidance must bump mapFollowRiderTick"
        )
    }

    func test_followRider_outsideRouting_isNoOp() {
        let app = AppModel()
        let vm = HomeViewModel(appModel: app)
        let before = vm.mapFollowRiderTick
        vm.notifyRiderLocationUpdated()
        XCTAssertEqual(
            vm.mapFollowRiderTick,
            before,
            "follow-tick must not bump outside phoneGuidance"
        )
    }

    // MARK: - Auto-recenter after user map interaction (spec line 104)

    func test_noteUserMapInteraction_duringRouting_schedulesRecenter() async {
        // Spec line 104: after the user pans/pinches/rotates during routing,
        // the camera smoothly returns to the default routing view after the
        // pinned inactivity timeout (1300 ms).
        let app = AppModel()
        let vm = HomeViewModel(appModel: app)
        vm.homeMode = .phoneGuidance
        let before = vm.mapRecenterRequestID
        vm.noteUserMapInteraction()
        // Before the timeout elapses: no extra bump.
        try? await Task.sleep(nanoseconds: 500_000_000)
        XCTAssertEqual(
            vm.mapRecenterRequestID,
            before,
            "recenter must not fire before the pinned timeout"
        )
        // After the full 1300 ms window (plus slack): one bump.
        try? await Task.sleep(nanoseconds: 1_000_000_000)
        XCTAssertGreaterThan(
            vm.mapRecenterRequestID,
            before,
            "recenter must fire after the pinned inactivity timeout (spec line 104)"
        )
    }

    func test_noteUserMapInteraction_resetsTimerOnSuccessiveCalls() async {
        let app = AppModel()
        let vm = HomeViewModel(appModel: app)
        vm.homeMode = .phoneGuidance
        let before = vm.mapRecenterRequestID
        vm.noteUserMapInteraction()
        try? await Task.sleep(nanoseconds: 1_000_000_000)
        vm.noteUserMapInteraction()
        try? await Task.sleep(nanoseconds: 1_000_000_000)
        XCTAssertEqual(
            vm.mapRecenterRequestID,
            before,
            "successive interactions must reset the timer so the recenter hasn't fired yet"
        )
        try? await Task.sleep(nanoseconds: 500_000_000)
        XCTAssertGreaterThan(
            vm.mapRecenterRequestID,
            before,
            "recenter should fire after the pinned timeout from the most-recent interaction"
        )
    }

    func test_noteUserMapInteraction_outsideRouting_isNoOp() async {
        let app = AppModel()
        let vm = HomeViewModel(appModel: app)
        let before = vm.mapRecenterRequestID
        vm.noteUserMapInteraction()
        try? await Task.sleep(nanoseconds: 1_500_000_000)
        XCTAssertEqual(
            vm.mapRecenterRequestID,
            before,
            "noteUserMapInteraction must not fire outside phoneGuidance"
        )
    }

    /// Regression for the MapKit feedback loop: `onMapCameraChange(.continuous)`
    /// fires many times during a programmatic animation. Each fire calls
    /// `noteUserMapInteraction`, which must coalesce to a single pending
    /// recenter (cancel-on-reentry), otherwise the camera keeps re-orienting
    /// and the bearing appears not to stick.
    func test_rapidSuccessive_noteUserMapInteraction_yieldsAtMostOneRecenter() async {
        let app = AppModel()
        let vm = HomeViewModel(appModel: app)
        vm.homeMode = .phoneGuidance
        let before = vm.mapRecenterRequestID
        // Simulate 20 rapid `onMapCameraChange` fires (what a 300 ms MapKit
        // animation generates in practice).
        for _ in 0..<20 {
            vm.noteUserMapInteraction()
        }
        try? await Task.sleep(nanoseconds: 1_600_000_000)
        XCTAssertEqual(
            vm.mapRecenterRequestID,
            before + 1,
            "rapid-fire noteUserMapInteraction must produce exactly one recenter, not one per call"
        )
    }

    // MARK: - Routing camera bearing (spec line 101)

    /// Helper that returns a 3-point L-shape route: start → 400 m north → 400 m east.
    /// Second leg is due east (~90°) so the segment-bearing change is obvious.
    private func lShapeRoute() -> NormalizedRoutePackage {
        let metersPerDegreeLat = 111_320.0
        let start = CoordinatePoint(latitude: 60.17, longitude: 24.94)
        let mid = CoordinatePoint(
            latitude: 60.17 + 400.0 / metersPerDegreeLat,
            longitude: 24.94
        )
        let cosLat = cos(60.17 * .pi / 180.0)
        let end = CoordinatePoint(
            latitude: mid.latitude,
            longitude: mid.longitude + 400.0 / (metersPerDegreeLat * cosLat)
        )
        return NormalizedRoutePackage(
            version: RoutePackageVersion.current,
            routeIdentifier: "bearing-test",
            revision: 1,
            geometry: [start, mid, end],
            maneuvers: [
                RouteManeuver(id: "m1", maneuverType: .depart, location: start,
                              distanceFromStartMeters: 0, distanceToNextMeters: 400, instructionText: nil),
                RouteManeuver(id: "m2", maneuverType: .right, location: mid,
                              distanceFromStartMeters: 400, distanceToNextMeters: 400, instructionText: nil),
                RouteManeuver(id: "m3", maneuverType: .arrive, location: end,
                              distanceFromStartMeters: 800, distanceToNextMeters: nil, instructionText: nil),
            ],
            summary: RouteSummary(totalDistanceMeters: 800, estimatedDurationSeconds: 240,
                                  startLabel: nil, destinationLabel: nil),
            provenance: RouteProvenance(providerID: .osm, sourceReference: nil, generatedAtUnixMs: 0)
        )
    }

    func test_routingBearingDegrees_atRouteStart_pointsAlongFirstLeg() async {
        let app = AppModel()
        let vm = HomeViewModel(appModel: app)
        let pkg = lShapeRoute()
        app.preview = RoutePreviewModel(
            alternatives: [RouteAlternative(
                id: UUID(), title: "R", subtitle: "",
                distanceMeters: 800, durationSeconds: 240, normalizedPackage: pkg
            )],
            selectedAlternativeID: nil, routeIdentifier: nil, routeRevision: nil, planningNotice: nil
        )
        await vm.startSelectedRoute()
        // At the start of the route: rider is at geometry[0].
        let bearing = vm.routingBearingDegrees(rider: pkg.geometry[0])
        XCTAssertNotNil(
            bearing,
            "HomeViewModel must expose routingBearingDegrees so the map can rotate along the route (spec line 101)"
        )
        // First leg points due north → 0°. Allow ±5°. Normalise to [-180, 180].
        let delta = ((bearing ?? 0.0) + 540.0).truncatingRemainder(dividingBy: 360.0) - 180.0
        XCTAssertLessThan(abs(delta), 5.0,
            "at route start the bearing should point along the first leg (north ≈ 0°), got \(bearing ?? 0)"
        )
    }

    func test_routingBearingDegrees_shiftsToNextLegOnceProgressPassesCorner() async {
        let app = AppModel()
        let vm = HomeViewModel(appModel: app)
        let pkg = lShapeRoute()
        app.preview = RoutePreviewModel(
            alternatives: [RouteAlternative(
                id: UUID(), title: "R", subtitle: "",
                distanceMeters: 800, durationSeconds: 240, normalizedPackage: pkg
            )],
            selectedAlternativeID: nil, routeIdentifier: nil, routeRevision: nil, planningNotice: nil
        )
        await vm.startSelectedRoute()
        // Rider at the corner — the "next segment" the rider is riding
        // towards is the east leg.
        let bearing = vm.routingBearingDegrees(rider: pkg.geometry[1])
        XCTAssertNotNil(bearing)
        // Second leg points due east → 90°. Allow ±5°.
        XCTAssertLessThan(abs((bearing ?? 0.0) - 90.0), 5.0,
            "after progress crosses the corner the bearing should point east (≈90°), got \(bearing ?? 0)"
        )
    }

    // MARK: - Compass lock holds the overview (web-parity regression)

    /// Spec lines 95-96 + web-parity regression: once the compass is locked,
    /// GPS ticks and user-interaction timeouts must NOT override the lock
    /// with a follow-rider camera. iOS achieves this at the view layer —
    /// `refreshCameraForCurrentMode` dispatches to `fitCamera(to:)` whenever
    /// `compassMode` is `.northLocked` or `.northPreview`, regardless of
    /// which tick changed. These tests lock in that invariant at the
    /// HomeViewModel layer by asserting the ticks keep flowing (so the view
    /// stays reactive) and the compass mode does not flip back to autoFollow
    /// from GPS / inactivity events.
    func test_compassLock_survivesRiderLocationUpdates() {
        let app = AppModel()
        let vm = HomeViewModel(appModel: app)
        vm.homeMode = .phoneGuidance
        vm.compassMode = .northLocked
        vm.notifyRiderLocationUpdated()
        vm.notifyRiderLocationUpdated()
        XCTAssertEqual(
            vm.compassMode,
            .northLocked,
            "GPS ticks must not flip compassMode off .northLocked"
        )
    }

    func test_compassLock_survivesInactivityTimeoutRecenter() async {
        let app = AppModel()
        let vm = HomeViewModel(appModel: app)
        vm.homeMode = .phoneGuidance
        vm.compassMode = .northLocked
        vm.noteUserMapInteraction()
        // Wait past the pinned inactivity timeout.
        try? await Task.sleep(nanoseconds: 1_500_000_000)
        XCTAssertEqual(
            vm.compassMode,
            .northLocked,
            "inactivity-timeout recenter must not flip compassMode off .northLocked"
        )
    }

    // MARK: - GPS-trail heading overrides route direction (spec line 110)

    /// Spec line 110 (authoritative): "camera rotates so that riding
    /// direction is towards top of the screen this overrides the camera
    /// of routing. Most important camera behaviour is this. (it needs to
    /// determine the direction by last few GPS locations it receives)".
    ///
    /// HeadingTrail is a small buffer of recent fixes + displacement floor
    /// + EMA smoothing. When it has enough motion to produce a heading,
    /// that heading WINS over the route-segment bearing. When it doesn't
    /// (stationary / first fix), routing falls back to the route bearing
    /// (spec line 101 "even when stationary yet").
    func test_headingTrail_jitterBelowFloor_yieldsNoHeading() {
        // An app that is "at rest" with ±0.5 m GPS wobble must not produce
        // a spinning camera heading.
        let trail = HeadingTrail(maxAgeMs: 5_000, maxFixes: 10, minDisplacementM: 3.0, smoothingAlpha: 0.25)
        let base = CoordinatePoint(latitude: 60.17, longitude: 24.94)
        trail.recordFix(base, timestampMs: 0)
        trail.recordFix(offset(base, eastM: 0.5, northM: 0.0), timestampMs: 100)
        XCTAssertNil(
            trail.travelHeadingDegrees,
            "tiny GPS jitter below the displacement floor must not produce a heading"
        )
    }

    func test_headingTrail_eastLeg_producesEastBearing() {
        let trail = HeadingTrail(maxAgeMs: 5_000, maxFixes: 10, minDisplacementM: 3.0, smoothingAlpha: 0.25)
        let base = CoordinatePoint(latitude: 60.17, longitude: 24.94)
        for i in 0..<5 {
            trail.recordFix(offset(base, eastM: Double(i) * 1.2, northM: 0.0), timestampMs: Int64(i) * 200)
        }
        let heading = trail.travelHeadingDegrees
        XCTAssertNotNil(heading)
        XCTAssertLessThan(abs((heading ?? 0.0) - 90.0), 5.0, "east-only motion should give ≈90°")
    }

    func test_headingTrail_smoothsLateralJitter() {
        let trail = HeadingTrail(maxAgeMs: 5_000, maxFixes: 10, minDisplacementM: 3.0, smoothingAlpha: 0.25)
        let base = CoordinatePoint(latitude: 60.17, longitude: 24.94)
        for i in 0..<20 {
            let east = Double(i) * 2.5
            let noise = (Double(i % 2) * 2.0 - 1.0) * 1.5
            trail.recordFix(offset(base, eastM: east, northM: noise), timestampMs: Int64(i) * 200)
        }
        let heading = trail.travelHeadingDegrees ?? 0.0
        XCTAssertLessThan(abs(heading - 90.0), 8.0,
            "smoothed trail heading must stay tight to east under lateral jitter")
    }

    func test_movingWithRoute_cameraBearingTracksTrailHeading_notRouteSegment() async {
        // Route segment points NORTH (first two geometry points have same longitude).
        // Rider is actually moving EAST. The camera bearing must match EAST, per spec 110.
        let app = AppModel()
        let vm = HomeViewModel(appModel: app)
        let start = CoordinatePoint(latitude: 60.17, longitude: 24.94)
        let northEnd = CoordinatePoint(latitude: 60.175, longitude: 24.94)
        let pkg = NormalizedRoutePackage(
            version: RoutePackageVersion.current,
            routeIdentifier: "north-route",
            revision: 1,
            geometry: [start, northEnd],
            maneuvers: [],
            summary: RouteSummary(totalDistanceMeters: 500, estimatedDurationSeconds: 120,
                                  startLabel: nil, destinationLabel: nil),
            provenance: RouteProvenance(providerID: .osm, sourceReference: nil, generatedAtUnixMs: 0)
        )
        app.preview = RoutePreviewModel(
            alternatives: [RouteAlternative(
                id: UUID(), title: "R", subtitle: "",
                distanceMeters: 500, durationSeconds: 120, normalizedPackage: pkg
            )],
            selectedAlternativeID: nil, routeIdentifier: nil, routeRevision: nil, planningNotice: nil
        )
        await vm.startSelectedRoute()
        // Drive 8 east-going fixes into the HomeViewModel's trail buffer.
        for i in 0..<8 {
            vm.ingestRiderLocationFix(offset(start, eastM: Double(i) * 2.5, northM: 0.0),
                                      timestampMs: Int64(i) * 200)
        }
        let heading = vm.cameraHeadingDegrees(rider: offset(start, eastM: 17.5, northM: 0.0))
        XCTAssertNotNil(heading)
        XCTAssertLessThan(abs((heading ?? 0.0) - 90.0), 8.0,
            "moving east while the route goes north — camera must follow east (GPS) per spec 110")
    }

    func test_movingWithoutRoute_cameraBearingTracksTrailHeading() {
        // No route at all. Rider is moving east. Camera heading must track east.
        let app = AppModel()
        let vm = HomeViewModel(appModel: app)
        let start = CoordinatePoint(latitude: 60.17, longitude: 24.94)
        for i in 0..<8 {
            vm.ingestRiderLocationFix(offset(start, eastM: Double(i) * 2.5, northM: 0.0),
                                      timestampMs: Int64(i) * 200)
        }
        let heading = vm.cameraHeadingDegrees(rider: offset(start, eastM: 17.5, northM: 0.0))
        XCTAssertNotNil(heading)
        XCTAssertLessThan(abs((heading ?? 0.0) - 90.0), 8.0,
            "moving east without a route — camera must still rotate to travel direction (spec 110)")
    }

    func test_stationaryOnRoute_cameraBearingFallsBackToRouteSegment() async {
        // Stationary rider on a north-pointing route. Camera heading must use
        // the route bearing (north ≈ 0°), per spec 101 "even when stationary yet".
        let app = AppModel()
        let vm = HomeViewModel(appModel: app)
        let start = CoordinatePoint(latitude: 60.17, longitude: 24.94)
        let northEnd = CoordinatePoint(latitude: 60.175, longitude: 24.94)
        let pkg = NormalizedRoutePackage(
            version: RoutePackageVersion.current,
            routeIdentifier: "north-route",
            revision: 1,
            geometry: [start, northEnd],
            maneuvers: [],
            summary: RouteSummary(totalDistanceMeters: 500, estimatedDurationSeconds: 120,
                                  startLabel: nil, destinationLabel: nil),
            provenance: RouteProvenance(providerID: .osm, sourceReference: nil, generatedAtUnixMs: 0)
        )
        app.preview = RoutePreviewModel(
            alternatives: [RouteAlternative(
                id: UUID(), title: "R", subtitle: "",
                distanceMeters: 500, durationSeconds: 120, normalizedPackage: pkg
            )],
            selectedAlternativeID: nil, routeIdentifier: nil, routeRevision: nil, planningNotice: nil
        )
        await vm.startSelectedRoute()
        // Feed the same location 5 times → no displacement, no trail heading.
        for i in 0..<5 {
            vm.ingestRiderLocationFix(start, timestampMs: Int64(i) * 200)
        }
        let heading = vm.cameraHeadingDegrees(rider: start)
        XCTAssertNotNil(heading)
        // First-segment bearing is 0° (due north). Allow ±5°.
        let h = heading ?? 0.0
        XCTAssertLessThan(abs(((h + 540.0).truncatingRemainder(dividingBy: 360.0)) - 180.0), 5.0,
            "stationary on a route — camera falls back to route bearing (north ≈ 0°)")
    }

    // Helpers.
    private func offset(_ base: CoordinatePoint, eastM: Double, northM: Double) -> CoordinatePoint {
        let metersPerDegreeLat = 111_320.0
        let meanLat = base.latitude * .pi / 180.0
        return CoordinatePoint(
            latitude: base.latitude + northM / metersPerDegreeLat,
            longitude: base.longitude + eastM / (metersPerDegreeLat * cos(meanLat))
        )
    }
}
