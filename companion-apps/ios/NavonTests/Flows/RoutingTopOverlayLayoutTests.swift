import XCTest
@testable import Navon

/// User-reported bug: during routing the device-pairing chip, the
/// alternative-routes button, and the north/user-view compass icon were
/// rendered inside the same `HStack` as the next-turn headline + subtitle,
/// with `.lineLimit(1)` and `.lineLimit(2)` on the text. With three icons
/// taking ~150 pt on the right, the headline text was visibly truncated
/// and the destination subtitle clipped to one or two letters.
///
/// The fix is to move the icons out of the top text card and into a
/// vertical side rail (so they live alongside the existing +/- zoom and
/// recenter icons). This regression test verifies the *layout intent*:
/// when the home is in `phoneGuidance` mode, `routingTopLayout` exposes
/// the headline + subtitle separately from the side-rail icons. The view
/// renders from `routingTopLayout`, so reverting the side-rail change
/// breaks the API contract this test pins down.
@MainActor
final class RoutingTopOverlayLayoutTests: XCTestCase {

    private func tinyRoute() -> NormalizedRoutePackage {
        let start = CoordinatePoint(latitude: 60.17, longitude: 24.94)
        let end = CoordinatePoint(latitude: 60.18, longitude: 24.95)
        return NormalizedRoutePackage(
            version: RoutePackageVersion.current,
            routeIdentifier: "tiny",
            revision: 1,
            geometry: [start, end],
            maneuvers: [
                RouteManeuver(id: "depart", maneuverType: .depart, location: start,
                              distanceFromStartMeters: 0, distanceToNextMeters: 1500, instructionText: nil),
                RouteManeuver(id: "arrive", maneuverType: .arrive, location: end,
                              distanceFromStartMeters: 1500, distanceToNextMeters: nil, instructionText: nil),
            ],
            summary: RouteSummary(totalDistanceMeters: 1500, estimatedDurationSeconds: 600,
                                  startLabel: nil, destinationLabel: "Helsinki Central Railway Station"),
            provenance: RouteProvenance(providerID: .osm, sourceReference: nil, generatedAtUnixMs: 0)
        )
    }

    func test_routingTopLayout_isNilOutsideOfPhoneGuidance() {
        let app = AppModel(
            persistence: CompanionPersistence(
                defaults: UserDefaults(suiteName: "routing-top-overlay-tests-\(UUID().uuidString)")!
            )
        )
        let vm = HomeViewModel(appModel: app)
        XCTAssertNil(vm.routingTopLayout, "Top layout is only meaningful in phoneGuidance mode")
    }

    func test_routingTopLayout_keepsIconsOutOfTheTopCard() async {
        // Spec follow-up: the icons (chip / alt-routes / compass) used to
        // share an HStack with the headline + subtitle inside the routing
        // top card, truncating the text. They were first moved to a per-
        // mode side rail, then to the persistent `topRightIconStack` /
        // `bottomRightIconStack` so the layout doesn't reflow between
        // modes. The contract here is just: the routing top card's
        // `sideRail` is empty — every icon lives in the right-side rails.
        let app = AppModel(
            persistence: CompanionPersistence(
                defaults: UserDefaults(suiteName: "routing-top-overlay-tests-\(UUID().uuidString)")!
            )
        )
        let vm = HomeViewModel(appModel: app)
        let pkg = tinyRoute()
        app.preview = RoutePreviewModel(
            alternatives: [RouteAlternative(
                id: UUID(), title: "T", subtitle: "",
                distanceMeters: 1500, durationSeconds: 600, normalizedPackage: pkg
            )],
            selectedAlternativeID: nil, routeIdentifier: nil, routeRevision: nil, planningNotice: nil
        )
        await vm.startSelectedRoute()
        let layout = vm.routingTopLayout
        XCTAssertNotNil(layout, "phoneGuidance must produce a routing top layout")
        XCTAssertEqual(layout?.sideRail, [],
                       "The routing top card must hold only text; icons live in topRightIconStack / bottomRightIconStack — got \(String(describing: layout?.sideRail))")
    }

    func test_routingTopLayout_topCardCarriesHeadlineAndSubtitle_withoutTruncationByIcons() async {
        // The original bug: a long destination ("Helsinki Central Railway
        // Station") was clipped to a few letters because the headline + the
        // 3 icons shared the same HStack with `.lineLimit(1)` and `.lineLimit(2)`.
        // The fix: the top card holds only text, so the full destination
        // can render across the row.
        let app = AppModel(
            persistence: CompanionPersistence(
                defaults: UserDefaults(suiteName: "routing-top-overlay-tests-\(UUID().uuidString)")!
            )
        )
        let vm = HomeViewModel(appModel: app)
        let pkg = tinyRoute()
        app.preview = RoutePreviewModel(
            alternatives: [RouteAlternative(
                id: UUID(), title: "T", subtitle: "",
                distanceMeters: 1500, durationSeconds: 600, normalizedPackage: pkg
            )],
            selectedAlternativeID: nil, routeIdentifier: nil, routeRevision: nil, planningNotice: nil
        )
        await vm.startSelectedRoute()
        let layout = vm.routingTopLayout
        XCTAssertNotNil(layout)
        XCTAssertTrue(
            (layout?.subtitle ?? "").contains("Helsinki Central Railway Station"),
            "Top card subtitle must include the full destination label — got '\(layout?.subtitle ?? "")'"
        )
    }
}
