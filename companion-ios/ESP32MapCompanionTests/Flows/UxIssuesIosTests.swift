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
        XCTAssertGreaterThanOrEqual(region.span.latitudeDelta, 0.014)
        XCTAssertGreaterThanOrEqual(region.span.longitudeDelta, 0.012)
    }

    func test_fittedRouteRegion_putsBboxCenterInUpperHalfOfVisible() {
        // Spec for the upward shift: the bbox center must sit ABOVE the
        // visible region's center (closer to the top of the screen) so
        // the bottom card doesn't crop the route. With the 0.22 *
        // latDelta southward shift, the bbox center should be roughly
        // in the upper third of the visible region.
        let region = HomeViewModel.fittedRouteRegion(
            minLat: 60.1700, maxLat: 60.1850,
            minLon: 24.9300, maxLon: 24.9380
        )
        let bboxCenterLat = (60.1700 + 60.1850) / 2.0
        let visibleSpan = region.span.latitudeDelta
        // bboxCenter relative to the visible region, where 0=bottom and
        // 1=top. Should land above 0.5 (upper half).
        let relative = (bboxCenterLat - (region.center.latitude - visibleSpan / 2.0)) / visibleSpan
        XCTAssertGreaterThan(relative, 0.6,
            "bbox center must sit in the upper portion of the visible region so the bottom card doesn't crop the route")
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

    // MARK: Recurring bug (2026-04-27): the route bbox must fit between the
    //                  top "Where to?" bar (~12% of screen) and the bottom
    //                  alternatives card (~50% of screen). Earlier 2.2× /
    //                  0.22 cropped the route start behind the card; bumping
    //                  to 2.6× / 0.30 over-corrected and pushed the route
    //                  end under the where-to bar at the top. The fit math
    //                  must satisfy BOTH constraints simultaneously.

    func test_fittedRouteRegion_bboxFitsBetweenTopBarAndBottomCard() {
        // Helsinki Kamppi → Töölö, ~1.7 km north-south route. Symmetric input
        // so any vertical asymmetry comes from the helper's intentional shift.
        let region = HomeViewModel.fittedRouteRegion(
            minLat: 60.1700, maxLat: 60.1850,
            minLon: 24.9300, maxLon: 24.9380
        )
        // Screen-y convention: 0 = top of screen, 1 = bottom.
        // Map's full vertical span maps linearly to the visible lat span.
        let visibleTopLat = region.center.latitude + region.span.latitudeDelta / 2.0
        let bboxTopScreenY = (visibleTopLat - 60.1850) / region.span.latitudeDelta
        let bboxBottomScreenY = (visibleTopLat - 60.1700) / region.span.latitudeDelta

        XCTAssertGreaterThan(
            bboxTopScreenY, 0.13,
            "bbox top must sit BELOW the top 'Where to?' bar (~12% of screen) with at least 1% buffer. Got screen-y \(bboxTopScreenY)."
        )
        XCTAssertLessThan(
            bboxBottomScreenY, 0.45,
            "bbox bottom must sit ABOVE the bottom alternatives card top (~45% from top, since the card can run ~55% on smaller iPhones). Got screen-y \(bboxBottomScreenY)."
        )
    }

    // MARK: Bug 2 + 3 (2026-04-27): in routing mode, the +/- icons should
    //                  preserve the rider's preferred zoom across compass
    //                  cycles + GPS-fix-driven camera refreshes — same as
    //                  companion-web's `settingsStore.settings.ridingZoom`.
    //                  Today the +/- buttons write `ridingCameraDistanceM`
    //                  to settings, but the autoFollow camera in
    //                  `orientCameraForTravel` hardcodes `distance: 1200`,
    //                  ignoring the saved value. The next GPS fix or
    //                  compass tap to autoFollow snaps the camera back to
    //                  1200, dropping the rider's preferred zoom. Then a
    //                  follow-up +/- tap multiplies from the SAVED value,
    //                  visually leaping past the displayed scale ("jumps
    //                  as if it was in the correct place").
    //
    // Fix: viewModel exposes `ridingCameraDistanceM` (the source of truth)
    // and `bumpRidingZoom(direction:)` (which writes through to settings).
    // The view's autoFollow camera reads `viewModel.ridingCameraDistanceM`
    // instead of a hardcoded constant.

    func test_ridingCameraDistanceM_defaultsTo1200_whenSettingUnset() {
        let app = AppModel()
        let vm = HomeViewModel(appModel: app)
        app.settings.ridingCameraDistanceM = nil
        XCTAssertEqual(
            vm.ridingCameraDistanceM, 1200, accuracy: 1e-6,
            "default routing-zoom distance must be 1200 m so first-run riders see a comfortable navigation scale."
        )
    }

    func test_ridingCameraDistanceM_readsFromPersistedOverride() {
        let app = AppModel()
        let vm = HomeViewModel(appModel: app)
        app.settings.ridingCameraDistanceM = 600
        XCTAssertEqual(
            vm.ridingCameraDistanceM, 600, accuracy: 1e-6,
            "viewModel must surface the persisted override so the autoFollow camera honours the rider's preferred zoom."
        )
    }

    func test_bumpRidingZoom_zoomIn_scalesDownBy1Over1Point5() {
        let app = AppModel()
        let vm = HomeViewModel(appModel: app)
        app.settings.ridingCameraDistanceM = 1200
        vm.bumpRidingZoom(direction: .zoomIn)
        XCTAssertEqual(
            vm.ridingCameraDistanceM, 1200.0 / 1.5, accuracy: 1e-6,
            "zoom-in must scale the saved distance by 1/1.5 (web parity, ~0.6 zoom levels per tap)."
        )
        XCTAssertEqual(
            app.settings.ridingCameraDistanceM ?? .nan, 1200.0 / 1.5, accuracy: 1e-6,
            "zoom-in must persist the new distance through settings so it survives relaunches."
        )
    }

    func test_bumpRidingZoom_zoomOut_clampsAt8000m() {
        let app = AppModel()
        let vm = HomeViewModel(appModel: app)
        app.settings.ridingCameraDistanceM = 7000
        vm.bumpRidingZoom(direction: .zoomOut)
        XCTAssertEqual(
            vm.ridingCameraDistanceM, 8000, accuracy: 1e-6,
            "zoom-out must clamp at 8000 m so the rider can't accidentally end up viewing the entire region from orbit."
        )
    }

    // MARK: Bug 2026-04-27 (b): in routing mode, deep zoom-in pushes the
    //                  rider off the bottom of the screen. The hardcoded 90 m
    //                  anchor offset in `cameraCenterCoordinate` doesn't
    //                  scale with camera distance — at D=300 (zoomed way
    //                  in), 90 m is a much larger fraction of the visible
    //                  map than at D=1200, so the rider drifts past the
    //                  bottom edge instead of staying in the bottom-quarter
    //                  anchor. Fix: anchor offset is linear in camera
    //                  distance, so the rider's screen-relative position
    //                  stays constant across zoom levels.

    func test_cameraCenterCoordinate_anchorOffsetScalesWithCameraDistance() {
        let app = AppModel()
        let vm = HomeViewModel(appModel: app)
        let rider = CoordinatePoint(latitude: 60.17, longitude: 24.94)

        // Heading north (0°): the anchor offset is purely in the latitude
        // direction so we can read it as `(center.lat - rider.lat) * mPerDeg`.
        let centerNear = vm.cameraCenterCoordinate(rider: rider, headingDegrees: 0.0, cameraDistanceM: 300)
        let centerFar = vm.cameraCenterCoordinate(rider: rider, headingDegrees: 0.0, cameraDistanceM: 1200)

        let offsetNearM = (centerNear.latitude - rider.latitude) * 111_320.0
        let offsetFarM = (centerFar.latitude - rider.latitude) * 111_320.0

        XCTAssertGreaterThan(offsetNearM, 0)
        XCTAssertEqual(
            offsetFarM / offsetNearM, 4.0, accuracy: 0.01,
            "4× camera distance must produce 4× anchor offset so the rider's screen-relative position is invariant under zoom."
        )
        XCTAssertEqual(
            offsetFarM, 90.0, accuracy: 0.5,
            "at the default 1200 m camera distance, the offset must equal the historical 90 m so the existing bottom-quarter anchor is preserved."
        )
    }

    func test_compassCycle_inRoutingMode_preservesRidingCameraDistance() async {
        // Spec line 39 + 95: in routing mode the compass single-tap cycles
        // autoFollow → northLocked → autoFollow. The rider's preferred
        // zoom (saved in `settings.ridingCameraDistanceM` by the +/-
        // buttons) must survive every transition — only entering / leaving
        // overview should NOT reset the saved value, since overview is a
        // session-only fit. Mirrors web `settingsStore.settings.ridingZoom`
        // which is read on every recenter without modification.
        let app = AppModel()
        let vm = HomeViewModel(appModel: app)
        app.preview = RoutePreviewModel(
            alternatives: [RouteAlternative(
                id: UUID(), title: "R1", subtitle: "",
                distanceMeters: 2500, durationSeconds: 600,
                normalizedPackage: Self.straightLinePackage()
            )],
            selectedAlternativeID: nil,
            routeIdentifier: nil,
            routeRevision: nil,
            planningNotice: nil
        )
        await vm.startSelectedRoute()
        XCTAssertEqual(vm.homeMode, .phoneGuidance, "precondition")

        // Rider has zoomed in to a tighter routing scale via the +/- buttons.
        app.settings.ridingCameraDistanceM = 600

        // Compass single-tap from autoFollow goes to .northLocked (overview).
        vm.handleCompassTap()
        XCTAssertEqual(vm.compassMode, .northLocked, "first compass tap should enter overview")

        // Compass single-tap from northLocked returns to autoFollow.
        vm.handleCompassTap()
        XCTAssertEqual(vm.compassMode, .autoFollow, "second compass tap should return to autoFollow")

        XCTAssertEqual(
            vm.ridingCameraDistanceM, 600, accuracy: 1e-6,
            "compass cycle must not reset the rider's preferred routing zoom — autoFollow's camera distance is the saved value, not a hardcoded default."
        )
    }
}
