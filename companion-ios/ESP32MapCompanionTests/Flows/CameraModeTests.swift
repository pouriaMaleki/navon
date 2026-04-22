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
            version: CURRENT_ROUTE_PACKAGE_VERSION,
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
        // also recenters the camera. Today `handleCompassTap` only mutates
        // `compassMode` and has no path into the map camera.
        //
        // Contract: HomeViewModel must expose a `mapRecenterRequestID`
        // property (plain `Int` or a `@Published` equivalent) that *bumps*
        // every time a recenter is requested. We read the property value via
        // `Mirror` before and after the tap and assert the value changed.
        // Merely adding the property without bumping it won't satisfy the
        // test — the invocation must be real.
        //
        // Expected RED until the property lands AND is updated inside
        // handleCompassTap.
        let app = AppModel()
        let vm = HomeViewModel(appModel: app)

        let before = readRecenterTick(vm)
        vm.handleCompassTap()
        let after = readRecenterTick(vm)

        XCTAssertNotNil(
            before,
            "HomeViewModel must expose `mapRecenterRequestID` (Int) so the map surface can observe recenter requests — spec line 39"
        )
        XCTAssertNotEqual(
            before,
            after,
            "handleCompassTap must bump mapRecenterRequestID on every tap (current value stayed at \(before ?? -1))"
        )
    }

    /// Reads a property named `mapRecenterRequestID` (or its `@Published`
    /// backing storage `_mapRecenterRequestID`) from `vm` via reflection.
    /// Returns nil if the property doesn't exist or isn't an Int.
    private func readRecenterTick(_ vm: Any) -> Int? {
        let mirror = Mirror(reflecting: vm)
        for child in mirror.children {
            let label = child.label ?? ""
            guard label == "mapRecenterRequestID" || label == "_mapRecenterRequestID" else {
                continue
            }
            if let value = child.value as? Int { return value }
            // `@Published` wraps the value one level deeper.
            let inner = Mirror(reflecting: child.value)
            for wrapped in inner.children {
                if wrapped.label == "value", let value = wrapped.value as? Int {
                    return value
                }
            }
        }
        return nil
    }
}
