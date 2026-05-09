import XCTest
@testable import ESP32MapCompanion

/// User-reported bug: audio cues never play during routing on iOS, even
/// with `audioCuesEnabled` and `allowBackgroundGps` both on. Root cause:
/// `HomeViewModel.advanceProgress` (called every GPS fix while routing)
/// did not dispatch a `CueSnapshot` into `routingActivityCoordinator`, so
/// the cue engine was never ticked.
///
/// These tests drive a real route, simulate GPS fixes that cross the 50 m
/// and 10 m approach thresholds, and assert that the speech port received
/// the corresponding phrases.
@MainActor
final class AudioCueDispatchTests: XCTestCase {

    final class SpeechSpy: SpeechPort {
        private(set) var spoken: [String] = []
        private(set) var lang: String = "en"
        var voiceAvailable: Bool = true
        func speak(_ text: String) { spoken.append(text) }
        func setLanguage(_ bcp47: String) { lang = bcp47 }
        func hasVoice(forLocale locale: String) -> Bool { voiceAvailable }
        func shutdown() {}
        func reset() { spoken = [] }
    }

    private func offset(_ base: CoordinatePoint, eastM: Double, northM: Double) -> CoordinatePoint {
        let metersPerDegLat = 111_320.0
        let meanLat = base.latitude * .pi / 180.0
        return CoordinatePoint(
            latitude: base.latitude + northM / metersPerDegLat,
            longitude: base.longitude + eastM / (metersPerDegLat * cos(meanLat))
        )
    }

    /// L-shape route: 400 m N then 400 m E. m2 (right turn) at 400 m.
    private func lShapeRoute() -> NormalizedRoutePackage {
        let metersPerDegLat = 111_320.0
        let start = CoordinatePoint(latitude: 60.17, longitude: 24.94)
        let mid = CoordinatePoint(
            latitude: 60.17 + 400.0 / metersPerDegLat,
            longitude: 24.94
        )
        let cosLat = cos(60.17 * .pi / 180.0)
        let end = CoordinatePoint(
            latitude: mid.latitude,
            longitude: mid.longitude + 400.0 / (metersPerDegLat * cosLat)
        )
        return NormalizedRoutePackage(
            version: RoutePackageVersion.current,
            routeIdentifier: "lshape-cues",
            revision: 1,
            geometry: [start, mid, end],
            maneuvers: [
                RouteManeuver(id: "m1", maneuverType: .depart, location: start,
                              distanceFromStartMeters: 0, distanceToNextMeters: 400, instructionText: nil),
                RouteManeuver(id: "m2", maneuverType: .right, location: mid,
                              distanceFromStartMeters: 400, distanceToNextMeters: 400, instructionText: "Turn right"),
                RouteManeuver(id: "m3", maneuverType: .arrive, location: end,
                              distanceFromStartMeters: 800, distanceToNextMeters: nil, instructionText: nil),
            ],
            summary: RouteSummary(totalDistanceMeters: 800, estimatedDurationSeconds: 240,
                                  startLabel: nil, destinationLabel: "Finish"),
            provenance: RouteProvenance(providerID: .osm, sourceReference: nil, generatedAtUnixMs: 0)
        )
    }

    private func makeApp(speech: SpeechPort) -> AppModel {
        let app = AppModel()
        app.replaceRoutingActivityCoordinatorForTesting(speech: speech)
        var s = app.settings
        s.audioCuesEnabled = true
        s.allowBackgroundGps = true
        // Spec line 144 default is "only in background", but these tests
        // assert cues fire on a synchronous tick — they don't drive the
        // scene phase. Disable the gate so the cue path is exercised.
        s.audioCuesOnlyInBackground = false
        // Pin metric + English so distance/direction assertions are
        // locale-independent on simulators with non-English system locale.
        s.distanceUnit = .metric
        s.language = .en
        app.settings = s
        return app
    }

    func test_firstTickAnnouncesNextTurn_replacingRouteStarted() async {
        // Per user feedback: "Route started" was useless padding. The first
        // sound the rider hears must be the actual next-turn announcement
        // (kind + distance), so they have something to plan for.
        let speech = SpeechSpy()
        let app = makeApp(speech: speech)
        let vm = HomeViewModel(appModel: app)
        let pkg = lShapeRoute()
        app.preview = RoutePreviewModel(
            alternatives: [RouteAlternative(
                id: UUID(), title: "L", subtitle: "",
                distanceMeters: 800, durationSeconds: 240, normalizedPackage: pkg
            )],
            selectedAlternativeID: nil, routeIdentifier: nil, routeRevision: nil, planningNotice: nil
        )
        await vm.startSelectedRoute()
        vm.ingestRiderLocationFix(pkg.geometry[0], timestampMs: 1_000)
        XCTAssertFalse(
            speech.spoken.contains("Route started"),
            "the legacy 'Route started' cue must not be spoken any more"
        )
        XCTAssertTrue(
            speech.spoken.contains(where: {
                $0.lowercased().contains("next turn") && $0.lowercased().contains("right")
            }),
            "first tick must announce the next turn (right) and a distance — got \(speech.spoken)"
        )
    }

    func test_turn50m_announced_whenCrossingApproachThreshold() async {
        let speech = SpeechSpy()
        let app = makeApp(speech: speech)
        let vm = HomeViewModel(appModel: app)
        let pkg = lShapeRoute()
        app.preview = RoutePreviewModel(
            alternatives: [RouteAlternative(
                id: UUID(), title: "L", subtitle: "",
                distanceMeters: 800, durationSeconds: 240, normalizedPackage: pkg
            )],
            selectedAlternativeID: nil, routeIdentifier: nil, routeRevision: nil, planningNotice: nil
        )
        await vm.startSelectedRoute()
        let start = pkg.geometry[0]
        // Tick at 100m up — distance to m2 (at 400m) = 300m → no 50m yet.
        vm.ingestRiderLocationFix(offset(start, eastM: 0, northM: 100), timestampMs: 1_000)
        XCTAssertFalse(speech.spoken.contains("In 50 meters, turn right"),
                       "50m cue must not fire 300m from the maneuver")
        // Tick at 360m up — distance to m2 = 40m → 50m cue fires.
        // The cue now carries the rider's actual distance (rounded to
        // nearest 10), not a hardcoded "50 meters" — at d ≈ 40 m we
        // expect the spoken phrase to read "In 40 meters, turn right".
        vm.ingestRiderLocationFix(offset(start, eastM: 0, northM: 360), timestampMs: 2_000)
        XCTAssertTrue(
            speech.spoken.contains("In 40 meters, turn right"),
            "Crossing the 50m threshold must speak the 50m cue with actual distance — got \(speech.spoken)"
        )
    }

    func test_rerouteResetsProgress_soFirstCueDoesNotDuplicate() async {
        // Bug: after a reroute, `progressDistanceM` from the old route was
        // never reset. On the first tick with the new routeId the CueEngine
        // saw stale progress and interpreted all new-route maneuvers as
        // "already passed", firing an "arriving at destination" ghost cue
        // instead of the correct orientation announcement. A second reroute
        // introduced a third spurious cue, and so on (N+1 cues after N reroutes).
        //
        // Fix: `advanceProgress` detects a routeId change and resets
        // progressDistanceM / routeTotalDistanceM before projecting.
        let metersPerDegLat = 111_320.0
        let speech = SpeechSpy()
        let app = makeApp(speech: speech)
        let vm = HomeViewModel(appModel: app)
        let pkg = lShapeRoute()
        app.preview = RoutePreviewModel(
            alternatives: [RouteAlternative(
                id: UUID(), title: "L", subtitle: "",
                distanceMeters: 800, durationSeconds: 240, normalizedPackage: pkg
            )],
            selectedAlternativeID: nil, routeIdentifier: nil, routeRevision: nil, planningNotice: nil
        )
        await vm.startSelectedRoute()

        // Advance rider 300m along the original route so progressDistanceM ≈ 300.
        let start = pkg.geometry[0]
        let point300m = CoordinatePoint(
            latitude: start.latitude + 300.0 / metersPerDegLat,
            longitude: start.longitude
        )
        vm.ingestRiderLocationFix(point300m, timestampMs: 1_000)
        speech.reset()

        // Simulate a reroute completing: new 200m route starting at point300m
        // with a right turn at 100m from the new start.
        let turnPoint = CoordinatePoint(
            latitude: point300m.latitude + 100.0 / metersPerDegLat,
            longitude: point300m.longitude
        )
        let reroutedEnd = CoordinatePoint(
            latitude: point300m.latitude + 200.0 / metersPerDegLat,
            longitude: point300m.longitude
        )
        let reroutedPkg = NormalizedRoutePackage(
            version: RoutePackageVersion.current,
            routeIdentifier: "lshape-rerouted",
            revision: 2,
            geometry: [point300m, turnPoint, reroutedEnd],
            maneuvers: [
                RouteManeuver(id: "r-m1", maneuverType: .depart, location: point300m,
                              distanceFromStartMeters: 0, distanceToNextMeters: 100, instructionText: nil),
                RouteManeuver(id: "r-m2", maneuverType: .right, location: turnPoint,
                              distanceFromStartMeters: 100, distanceToNextMeters: 100, instructionText: "Turn right"),
                RouteManeuver(id: "r-m3", maneuverType: .arrive, location: reroutedEnd,
                              distanceFromStartMeters: 200, distanceToNextMeters: nil, instructionText: nil),
            ],
            summary: RouteSummary(totalDistanceMeters: 200, estimatedDurationSeconds: 60,
                                  startLabel: nil, destinationLabel: "Finish"),
            provenance: RouteProvenance(providerID: .osm, sourceReference: nil, generatedAtUnixMs: 0)
        )
        app.preview = RoutePreviewModel(
            alternatives: [RouteAlternative(
                id: UUID(), title: "Rerouted", subtitle: "",
                distanceMeters: 200, durationSeconds: 60, normalizedPackage: reroutedPkg
            )],
            selectedAlternativeID: nil, routeIdentifier: nil, routeRevision: nil, planningNotice: nil
        )
        app.activeSession.routeIdentifier = "lshape-rerouted"

        // First GPS fix on the new route — rider is at its very start.
        vm.ingestRiderLocationFix(point300m, timestampMs: 2_000)

        // Should speak the orientation cue ONCE. Without the progress reset,
        // the engine would see all new-route maneuvers as "passed" and fire a
        // ghost "arriving at destination" cue (or additional duplicate turn cues).
        let arrivingCues = speech.spoken.filter { $0.lowercased().contains("arriving") }
        XCTAssertTrue(
            arrivingCues.isEmpty,
            "must not fire a ghost 'arriving' cue on reroute — got \(speech.spoken)"
        )
        let nextTurnCues = speech.spoken.filter {
            $0.lowercased().contains("next turn") && $0.lowercased().contains("right")
        }
        XCTAssertEqual(
            nextTurnCues.count, 1,
            "after reroute, orientation cue must fire exactly once — got \(speech.spoken)"
        )
    }

    func test_rerouteWithSameRouteId_differentRevision_resetsProgressAndCueEngine() async {
        // Bug: when rerouting returns the same routeIdentifier (same origin +
        // destination, same provider path) but a higher revision, the
        // routeId-change guard in advanceProgress was never triggered because
        // the identifier string didn't change. The CueEngine's
        // routeStartedAnnounced flag stayed true and progressDistanceM wasn't
        // reset, so the new route's maneuvers all appeared "already passed" and
        // a ghost arrivingInM cue fired instead of the correct orientation cue.
        let metersPerDegLat = 111_320.0
        let speech = SpeechSpy()
        let app = makeApp(speech: speech)
        let vm = HomeViewModel(appModel: app)
        let pkg = lShapeRoute()  // routeIdentifier = "lshape-cues", revision 1
        app.preview = RoutePreviewModel(
            alternatives: [RouteAlternative(
                id: UUID(), title: "L", subtitle: "",
                distanceMeters: 800, durationSeconds: 240, normalizedPackage: pkg
            )],
            selectedAlternativeID: nil, routeIdentifier: nil, routeRevision: nil, planningNotice: nil
        )
        await vm.startSelectedRoute()

        let start = pkg.geometry[0]
        let point300m = CoordinatePoint(
            latitude: start.latitude + 300.0 / metersPerDegLat,
            longitude: start.longitude
        )
        vm.ingestRiderLocationFix(point300m, timestampMs: 1_000)
        speech.reset()

        // Reroute with the SAME routeIdentifier but revision bumped to 2.
        let turnPoint = CoordinatePoint(
            latitude: point300m.latitude + 100.0 / metersPerDegLat,
            longitude: point300m.longitude
        )
        let reroutedEnd = CoordinatePoint(
            latitude: point300m.latitude + 200.0 / metersPerDegLat,
            longitude: point300m.longitude
        )
        let reroutedPkg = NormalizedRoutePackage(
            version: RoutePackageVersion.current,
            routeIdentifier: "lshape-cues",  // same identifier as original
            revision: 2,                      // but bumped revision
            geometry: [point300m, turnPoint, reroutedEnd],
            maneuvers: [
                RouteManeuver(id: "r-m1", maneuverType: .depart, location: point300m,
                              distanceFromStartMeters: 0, distanceToNextMeters: 100, instructionText: nil),
                RouteManeuver(id: "r-m2", maneuverType: .right, location: turnPoint,
                              distanceFromStartMeters: 100, distanceToNextMeters: 100, instructionText: "Turn right"),
                RouteManeuver(id: "r-m3", maneuverType: .arrive, location: reroutedEnd,
                              distanceFromStartMeters: 200, distanceToNextMeters: nil, instructionText: nil),
            ],
            summary: RouteSummary(totalDistanceMeters: 200, estimatedDurationSeconds: 60,
                                  startLabel: nil, destinationLabel: "Finish"),
            provenance: RouteProvenance(providerID: .osm, sourceReference: nil, generatedAtUnixMs: 0)
        )
        app.preview = RoutePreviewModel(
            alternatives: [RouteAlternative(
                id: UUID(), title: "Rerouted", subtitle: "",
                distanceMeters: 200, durationSeconds: 60, normalizedPackage: reroutedPkg
            )],
            selectedAlternativeID: nil, routeIdentifier: nil, routeRevision: nil, planningNotice: nil
        )
        app.activeSession.routeIdentifier = "lshape-cues"
        app.activeSession.routeRevision = 2

        vm.ingestRiderLocationFix(point300m, timestampMs: 2_000)

        let arrivingCues = speech.spoken.filter { $0.lowercased().contains("arriving") }
        XCTAssertTrue(
            arrivingCues.isEmpty,
            "same-routeId reroute with bumped revision must not fire ghost arrivingInM — got \(speech.spoken)"
        )
        let nextTurnCues = speech.spoken.filter {
            $0.lowercased().contains("next turn") && $0.lowercased().contains("right")
        }
        XCTAssertEqual(
            nextTurnCues.count, 1,
            "after same-routeId reroute with new revision, orientation cue must fire once — got \(speech.spoken)"
        )
    }

    func test_connectedToDevice_suppressesAllCues() async {
        // Spec line 131: cues are suppressed only when the companion is
        // ACTIVELY CONNECTED to the ESP (the device is then driving the
        // on-screen UI). A paired-but-disconnected peripheral does NOT
        // suppress — covered by `CueDeviceGatingTests`.
        let speech = SpeechSpy()
        let app = makeApp(speech: speech)
        app.replacePairedPeripheralForTesting(
            PairedPeripheralRecord(
                identifier: "AA:BB", friendlyName: "Bike", pairedAt: Date()
            )
        )
        app.replaceDeviceConnectedForTesting(true)
        let vm = HomeViewModel(appModel: app)
        let pkg = lShapeRoute()
        app.preview = RoutePreviewModel(
            alternatives: [RouteAlternative(
                id: UUID(), title: "L", subtitle: "",
                distanceMeters: 800, durationSeconds: 240, normalizedPackage: pkg
            )],
            selectedAlternativeID: nil, routeIdentifier: nil, routeRevision: nil, planningNotice: nil
        )
        await vm.startSelectedRoute()
        vm.ingestRiderLocationFix(pkg.geometry[0], timestampMs: 1_000)
        XCTAssertEqual(
            speech.spoken, [],
            "Cues must be silent when actively connected to a device — got \(speech.spoken)"
        )
    }
}
