import MapKit
import XCTest
@testable import ESP32MapCompanion

/// Coverage for the four iOS UX bugs reported on 2026-04-27 (the "these"
/// list in the parallel slack thread):
///
///   1. Suggested-routes card sits on top of the map polylines (the
///      fitCamera region was symmetric, so the bbox was always centered
///      on screen and the bottom card cropped it).
///   2. +/- icons sat on top of the planning north-up / recenter button.
///   3. After Start, +/- icons covered the next-turn top card and didn't
///      respond to taps because the card stole the touch.
///   4. Split-way "alternate routes from here" was not implemented at all
///      on iOS — there was no entrypoint and no viewModel method.
///
/// Issues 1-3 are pure view-layout (overlay z-order / paddings / region
/// math) — only the math is testable as pure code; the SwiftUI
/// positioning is verified by visual inspection on a device. Issue 4 is
/// state-machine logic and is fully covered here.
@MainActor
final class UxIssuesIosTests: XCTestCase {

    // MARK: Issue #1 — fittedRouteRegion shifts the bbox into the upper
    //                  ~65% of the visible region so the bottom card
    //                  doesn't crop it.

    func test_fittedRouteRegion_shiftsCenterSouthOfBboxCenter() {
        // bbox: ~Helsinki Kamppi → Töölö, lat span ≈ 0.0150°, lon span
        // ≈ 0.0080°. Symmetric input — the asymmetry must come from the
        // helper's intentional camera-center shift.
        let region = HomeViewModel.fittedRouteRegion(
            minLat: 60.1700, maxLat: 60.1850,
            minLon: 24.9300, maxLon: 24.9380
        )
        let bboxCenterLat = (60.1700 + 60.1850) / 2.0
        XCTAssertLessThan(
            region.center.latitude, bboxCenterLat,
            "Camera center must sit south of bbox center so the route renders in the upper portion of the visible map."
        )
        // Longitude is left untouched: there's no horizontal UI overlay
        // to compensate for.
        let bboxCenterLon = (24.9300 + 24.9380) / 2.0
        XCTAssertEqual(region.center.longitude, bboxCenterLon, accuracy: 1e-9)
    }

    func test_fittedRouteRegion_keepsBboxFullyInsideVisibleRegion() {
        // Even after the southward shift, the visible region must still
        // cover the entire bbox. Otherwise we'd crop the route at the
        // top edge and lose the destination.
        let minLat = 60.1700
        let maxLat = 60.1850
        let minLon = 24.9300
        let maxLon = 24.9380
        let region = HomeViewModel.fittedRouteRegion(
            minLat: minLat, maxLat: maxLat,
            minLon: minLon, maxLon: maxLon
        )
        let halfLat = region.span.latitudeDelta / 2.0
        let halfLon = region.span.longitudeDelta / 2.0
        XCTAssertLessThanOrEqual(region.center.latitude - halfLat, minLat,
            "Visible bottom edge must reach minLat or further south.")
        XCTAssertGreaterThanOrEqual(region.center.latitude + halfLat, maxLat,
            "Visible top edge must reach maxLat or further north.")
        XCTAssertLessThanOrEqual(region.center.longitude - halfLon, minLon)
        XCTAssertGreaterThanOrEqual(region.center.longitude + halfLon, maxLon)
    }

    func test_fittedRouteRegion_zeroSpanFallsBackToMinimumDelta() {
        // Single point (minLat == maxLat) — the function must still
        // produce a usable visible region rather than collapsing to a
        // pinhole. The minimum delta keeps the rider context visible
        // around the destination marker.
        let region = HomeViewModel.fittedRouteRegion(
            minLat: 60.1699, maxLat: 60.1699,
            minLon: 24.9384, maxLon: 24.9384
        )
        XCTAssertGreaterThanOrEqual(region.span.latitudeDelta, 0.012)
        XCTAssertGreaterThanOrEqual(region.span.longitudeDelta, 0.01)
    }

    // MARK: Issue #4 — exploreAlternateRoutes drops out of phoneGuidance
    //                  back to planning, hands a fresh
    //                  origin = riderLocation request to AppModel, and
    //                  preserves the destination from the active session.

    func test_exploreAlternateRoutes_outsidePhoneGuidance_isNoOp() {
        let app = AppModel()
        let vm = HomeViewModel(appModel: app)
        XCTAssertEqual(vm.homeMode, .planning)
        vm.exploreAlternateRoutes()
        // Without a phoneGuidance precondition the call is rejected so
        // the rider can't accidentally rebuild the suggestions card from
        // an empty planning state and lose their typed query.
        XCTAssertEqual(vm.homeMode, .planning)
    }

    func test_exploreAlternateRoutes_inPhoneGuidance_switchesToPlanning() async {
        let app = AppModel()
        let vm = HomeViewModel(appModel: app)
        let pkg = Self.straightLinePackage()
        app.preview = RoutePreviewModel(
            alternatives: [RouteAlternative(
                id: UUID(), title: "R1", subtitle: "",
                distanceMeters: 2500, durationSeconds: 600,
                normalizedPackage: pkg
            )],
            selectedAlternativeID: nil,
            routeIdentifier: nil,
            routeRevision: nil,
            planningNotice: nil
        )
        await vm.startSelectedRoute()
        XCTAssertEqual(vm.homeMode, .phoneGuidance, "precondition")
        // Capture the destination at the moment of the split-way tap so we
        // can verify the freshly-issued routeRequest carries it forward.
        let destination = app.activeSession.destinationCoordinate

        vm.exploreAlternateRoutes()

        XCTAssertEqual(vm.homeMode, .planning,
            "Tapping the split-way icon must drop guidance back to planning so the alternatives card and route-overview camera take over.")
        XCTAssertEqual(vm.compassMode, .autoFollow,
            "compassMode must reset (north-preview lock from the prior trip would otherwise leak into the new selection).")
        XCTAssertEqual(app.routeRequest.destination, destination,
            "Destination must be preserved verbatim — the user is asking for alternatives to the SAME destination.")
        // origin should be the rider's current location, not the original
        // start point. Without GPS it falls back to the static rider
        // fallback (defaultRiderFallback).
        XCTAssertEqual(app.routeRequest.origin, AppModel.defaultRiderFallback,
            "origin must be the rider's current location at the moment of the request.")
    }

    private static func straightLinePackage() -> NormalizedRoutePackage {
        let origin = CoordinatePoint(latitude: 60.1699, longitude: 24.9384)
        let destination = CoordinatePoint(latitude: 60.1921, longitude: 24.9458)
        return NormalizedRoutePackage(
            version: RoutePackageVersion.current,
            routeIdentifier: "osm-straight",
            revision: 1,
            geometry: [origin, destination],
            maneuvers: [
                RouteManeuver(
                    id: "m1", maneuverType: .depart, location: origin,
                    distanceFromStartMeters: 0, distanceToNextMeters: 2500,
                    instructionText: "Depart"
                ),
                RouteManeuver(
                    id: "m2", maneuverType: .arrive, location: destination,
                    distanceFromStartMeters: 2500, distanceToNextMeters: nil,
                    instructionText: "Arrive"
                )
            ],
            summary: RouteSummary(
                totalDistanceMeters: 2500, estimatedDurationSeconds: 600,
                startLabel: nil, destinationLabel: "Töölö"
            ),
            provenance: RouteProvenance(
                providerID: .osm, sourceReference: nil, generatedAtUnixMs: 0
            )
        )
    }
}
