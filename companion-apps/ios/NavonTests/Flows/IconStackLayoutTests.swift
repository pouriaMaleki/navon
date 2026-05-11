import XCTest
@testable import Navon

/// User-reported: rail icons used to reflow as the home mode changed,
/// confusing the rider. The settled rule:
///
/// **Both rails are anchored to a fixed Y offset across every home mode**
/// (the offset is sized to clear the tallest possible top card — the
/// routing next-turn / destination / ETA card with an optional off-route
/// pill on top). Rail items appear in a fixed top-down order; items that
/// are conditional always sit at the BOTTOM of their list so the items
/// above never shift when the conditional ones appear or disappear.
///
/// **Top-RIGHT column (top → bottom)**
///   1. Settings (always)
///   2. Compass / north-up (always; tap = north-up, double-tap = lock,
///      matching the Rust impl)
///   3. Device-pairing chip (only when there is a paired peripheral)
///
/// **Top-LEFT column (top → bottom)**
///   1. Zoom-in (always)
///   2. Zoom-out (always)
///   3. Alternate-routes / split icon (only in `phoneGuidance`)
///
/// `topBar` (the where-to search field) no longer carries the Settings
/// cog — the bar is full-width and Settings lives on the rail.
@MainActor
final class IconStackLayoutTests: XCTestCase {

    private func tinyRoute() -> NormalizedRoutePackage {
        let metersPerDegLat = 111_320.0
        let start = CoordinatePoint(latitude: 60.17, longitude: 24.94)
        let mid = CoordinatePoint(
            latitude: 60.17 + 400.0 / metersPerDegLat,
            longitude: 24.94
        )
        return NormalizedRoutePackage(
            version: RoutePackageVersion.current,
            routeIdentifier: "icons-test",
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

    private func startedRouting() async -> (AppModel, HomeViewModel) {
        let app = AppModel()
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
        return (app, vm)
    }

    // MARK: top-right rail — settings always first

    func test_topRightIconStack_inPlanningMode_isSettingsThenCompass_whenUnpaired() {
        let app = AppModel()
        let vm = HomeViewModel(appModel: app)
        XCTAssertEqual(
            vm.topRightIconStack, [.settings, .compass],
            "Settings must always sit at the very top of the right rail so its on-screen position never moves between modes."
        )
    }

    func test_topRightIconStack_inPlanningMode_appendsDeviceChipAtBottom_whenPaired() {
        let app = AppModel()
        app.replacePairedPeripheralForTesting(
            PairedPeripheralRecord(identifier: "AA", friendlyName: "Bike", pairedAt: Date())
        )
        let vm = HomeViewModel(appModel: app)
        XCTAssertEqual(
            vm.topRightIconStack, [.settings, .compass, .deviceChip],
            "Conditional .deviceChip always appears at the BOTTOM of the right rail so the always-on items above don't shift when it shows up."
        )
    }

    func test_topRightIconStack_inRoutingMode_keepsSameOrder_asPlanningMode() async {
        let (_, vm) = await startedRouting()
        XCTAssertEqual(
            vm.topRightIconStack, [.settings, .compass],
            "The icon order in routing must match planning so the user's mental map (settings → compass → chip) is stable."
        )
    }

    // MARK: top-left rail — zoom always first; alt-routes appended at bottom

    func test_topLeftIconStack_inPlanningMode_isZoomInThenZoomOut() {
        let app = AppModel()
        let vm = HomeViewModel(appModel: app)
        XCTAssertEqual(
            vm.topLeftIconStack, [.zoomIn, .zoomOut],
            "Zoom-in/out must sit at the top of the left rail so their positions never change."
        )
    }

    func test_topLeftIconStack_inRoutingMode_appendsAlternateRoutesAtBottom() async {
        let (_, vm) = await startedRouting()
        XCTAssertEqual(
            vm.topLeftIconStack, [.zoomIn, .zoomOut, .alternateRoutes],
            "Pressing Start must NOT shift the zoom buttons. Alternate-routes appears at the bottom of the rail so zoom stays put."
        )
    }

    func test_topLeftIconStack_zoomIconPositions_doNotChangeBetweenPlanningAndRouting() async {
        let (_, vm) = await startedRouting()
        let routingIndexZoomIn = vm.topLeftIconStack.firstIndex(of: .zoomIn)
        let routingIndexZoomOut = vm.topLeftIconStack.firstIndex(of: .zoomOut)
        XCTAssertEqual(routingIndexZoomIn, 0,
                       "Zoom-in must remain at index 0 in routing mode (no shift when alt-routes appears).")
        XCTAssertEqual(routingIndexZoomOut, 1,
                       "Zoom-out must remain at index 1 in routing mode.")
    }

    // MARK: top bar — settings is no longer inline with where-to

    func test_topRightIconStack_alwaysIncludesSettings_acrossEveryMode() async {
        let app = AppModel()
        let vm = HomeViewModel(appModel: app)
        XCTAssertTrue(vm.topRightIconStack.contains(.settings),
                      "Settings must live on the right rail in EVERY mode (including planning) so the where-to bar can use the full top width.")
        let pkg = tinyRoute()
        app.preview = RoutePreviewModel(
            alternatives: [RouteAlternative(
                id: UUID(), title: "T", subtitle: "",
                distanceMeters: 400, durationSeconds: 120, normalizedPackage: pkg
            )],
            selectedAlternativeID: nil, routeIdentifier: nil, routeRevision: nil, planningNotice: nil
        )
        await vm.startSelectedRoute()
        XCTAssertTrue(vm.topRightIconStack.contains(.settings),
                      "Settings must remain on the right rail in routing mode too.")
    }
}
