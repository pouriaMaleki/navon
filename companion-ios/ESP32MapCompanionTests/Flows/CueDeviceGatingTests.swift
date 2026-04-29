import XCTest
@testable import ESP32MapCompanion

/// User-reported: audio cues did not fire while a paired device existed,
/// even when the device wasn't actually connected. The spec says cues are
/// suppressed only when the companion is **connected** to the device
/// (because the device is then driving the on-screen guidance). A paired
/// peripheral that's currently disconnected should still let the phone
/// speak cues — the rider is using the phone alone.
@MainActor
final class CueDeviceGatingTests: XCTestCase {

    private func tinyRoute() -> NormalizedRoutePackage {
        let metersPerDegLat = 111_320.0
        let start = CoordinatePoint(latitude: 60.17, longitude: 24.94)
        let mid = CoordinatePoint(
            latitude: 60.17 + 400.0 / metersPerDegLat,
            longitude: 24.94
        )
        return NormalizedRoutePackage(
            version: RoutePackageVersion.current,
            routeIdentifier: "device-gate",
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

    func test_pairedButDisconnected_doesNotSuppressAudioCues() async {
        // The user has a peripheral remembered (paired) but the BLE link
        // is currently down — the phone is the only UI. Cues must fire.
        let speech = AudioCueDispatchTests.SpeechSpy()
        let app = AppModel()
        app.replaceRoutingActivityCoordinatorForTesting(speech: speech)
        app.replacePairedPeripheralForTesting(
            PairedPeripheralRecord(identifier: "AA", friendlyName: "Bike", pairedAt: Date())
        )
        app.replaceDeviceConnectedForTesting(false)
        var s = app.settings
        s.audioCuesEnabled = true
        s.allowBackgroundGps = true
        app.settings = s

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
        vm.ingestRiderLocationFix(pkg.geometry[0], timestampMs: 1_000)

        XCTAssertTrue(
            speech.spoken.contains("Route started"),
            "Paired-but-disconnected must not suppress cues — got \(speech.spoken)"
        )
    }

    func test_pairedAndConnected_suppressesAudioCues() async {
        // The device is the on-screen UI when actively connected, so the
        // phone must stay silent (spec: companion app cues fire only when
        // NOT connected to a device).
        let speech = AudioCueDispatchTests.SpeechSpy()
        let app = AppModel()
        app.replaceRoutingActivityCoordinatorForTesting(speech: speech)
        app.replacePairedPeripheralForTesting(
            PairedPeripheralRecord(identifier: "AA", friendlyName: "Bike", pairedAt: Date())
        )
        app.replaceDeviceConnectedForTesting(true)
        var s = app.settings
        s.audioCuesEnabled = true
        s.allowBackgroundGps = true
        app.settings = s

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
        vm.ingestRiderLocationFix(pkg.geometry[0], timestampMs: 1_000)

        XCTAssertEqual(
            speech.spoken, [],
            "Cues must be silent when actively connected to a device — got \(speech.spoken)"
        )
    }
}
