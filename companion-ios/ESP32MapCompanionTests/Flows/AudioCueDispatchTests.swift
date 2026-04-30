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
        app.settings = s
        return app
    }

    func test_routeStarted_announced_onFirstTickOfRouting() async {
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
        // Feed a single fix at the start so a guidance tick fires.
        vm.ingestRiderLocationFix(pkg.geometry[0], timestampMs: 1_000)
        XCTAssertTrue(
            speech.spoken.contains("Route started"),
            "Audio cue 'Route started' must be spoken on the first guidance tick — got \(speech.spoken)"
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
        vm.ingestRiderLocationFix(offset(start, eastM: 0, northM: 360), timestampMs: 2_000)
        XCTAssertTrue(
            speech.spoken.contains("In 50 meters, turn right"),
            "Crossing the 50m threshold must speak the 50m cue — got \(speech.spoken)"
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
