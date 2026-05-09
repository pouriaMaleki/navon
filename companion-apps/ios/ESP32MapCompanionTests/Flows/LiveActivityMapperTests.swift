import XCTest
@testable import ESP32MapCompanion

final class LiveActivityMapperTests: XCTestCase {

    // MARK: - Glyph mapping

    func test_atomicLeftMaps() {
        XCTAssertEqual(LiveActivityMapper.glyph(primary: .left, followUp: nil, gapMeters: nil), .left)
    }

    func test_atomicSlightLeftMaps() {
        XCTAssertEqual(LiveActivityMapper.glyph(primary: .slightLeft, followUp: nil, gapMeters: nil), .slightLeft)
    }

    func test_atomicSharpRightMaps() {
        XCTAssertEqual(LiveActivityMapper.glyph(primary: .sharpRight, followUp: nil, gapMeters: nil), .sharpRight)
    }

    func test_atomicUturnMaps() {
        XCTAssertEqual(LiveActivityMapper.glyph(primary: .uturn, followUp: nil, gapMeters: nil), .uturn)
    }

    func test_atomicArriveMaps() {
        XCTAssertEqual(LiveActivityMapper.glyph(primary: .arrive, followUp: nil, gapMeters: nil), .arrive)
    }

    func test_atomicRoundaboutMaps() {
        XCTAssertEqual(LiveActivityMapper.glyph(primary: .roundabout, followUp: nil, gapMeters: nil), .roundabout)
    }

    func test_compoundLeftThenRightWhenGap20m() {
        XCTAssertEqual(
            LiveActivityMapper.glyph(primary: .left, followUp: .right, gapMeters: 20),
            .leftThenRight
        )
    }

    func test_compoundRightThenLeftWhenGapAtThreshold30m() {
        // Threshold is inclusive — mirrors CueEngine.backToBackThresholdM == 30 m.
        XCTAssertEqual(
            LiveActivityMapper.glyph(primary: .right, followUp: .left, gapMeters: 30),
            .rightThenLeft
        )
    }

    func test_compoundLeftThenLeftFromSlightVariants() {
        // slight/sharp left both fold into the "left" family for compound glyph.
        XCTAssertEqual(
            LiveActivityMapper.glyph(primary: .slightLeft, followUp: .sharpLeft, gapMeters: 25),
            .leftThenLeft
        )
    }

    func test_atomicWhenGapExceedsThreshold() {
        // gap > 30 m means two distinct turns; render only the upcoming one.
        XCTAssertEqual(
            LiveActivityMapper.glyph(primary: .left, followUp: .right, gapMeters: 31),
            .left
        )
    }

    func test_atomicWhenFollowUpNil() {
        XCTAssertEqual(
            LiveActivityMapper.glyph(primary: .right, followUp: nil, gapMeters: nil),
            .right
        )
    }

    func test_atomicWhenFollowUpNotInLRFamily() {
        // Roundabout / merge / ramp don't compose into a 2-arrow glyph; fall back to atomic.
        XCTAssertEqual(
            LiveActivityMapper.glyph(primary: .left, followUp: .roundabout, gapMeters: 10),
            .left
        )
    }

    func test_atomicWhenPrimaryNotInLRFamily() {
        XCTAssertEqual(
            LiveActivityMapper.glyph(primary: .roundabout, followUp: .left, gapMeters: 10),
            .roundabout
        )
    }

    func test_allRouteManeuverTypesMapExhaustively() {
        // No fatalError / unhandled case — every RouteManeuverType produces a glyph.
        let allTypes: [RouteManeuverType] = [
            .depart, .straight, .slightLeft, .left, .sharpLeft,
            .slightRight, .right, .sharpRight,
            .uturn, .roundabout, .merge, .ramp, .arrive
        ]
        for t in allTypes {
            _ = LiveActivityMapper.glyph(primary: t, followUp: nil, gapMeters: nil)
        }
    }

    // MARK: - ContentState

    private func route(
        totalM: Double = 1000,
        durationS: Int = 600,
        maneuvers: [RouteManeuver]? = nil
    ) -> NormalizedRoutePackage {
        let m = maneuvers ?? [
            RouteManeuver(id: "m0", maneuverType: .depart,
                          location: .init(latitude: 0, longitude: 0),
                          distanceFromStartMeters: 0,
                          distanceToNextMeters: nil, instructionText: nil),
            RouteManeuver(id: "m1", maneuverType: .left,
                          location: .init(latitude: 0, longitude: 0),
                          distanceFromStartMeters: 200,
                          distanceToNextMeters: nil, instructionText: nil),
            RouteManeuver(id: "m2", maneuverType: .right,
                          location: .init(latitude: 0, longitude: 0),
                          distanceFromStartMeters: 600,
                          distanceToNextMeters: nil, instructionText: nil),
            RouteManeuver(id: "m3", maneuverType: .arrive,
                          location: .init(latitude: 0, longitude: 0),
                          distanceFromStartMeters: 1000,
                          distanceToNextMeters: nil, instructionText: nil),
        ]
        return NormalizedRoutePackage(
            version: .current,
            routeIdentifier: "test-route",
            revision: 1,
            geometry: [.init(latitude: 0, longitude: 0), .init(latitude: 0.001, longitude: 0)],
            maneuvers: m,
            summary: RouteSummary(
                totalDistanceMeters: totalM,
                estimatedDurationSeconds: durationS,
                startLabel: nil,
                destinationLabel: nil
            ),
            provenance: RouteProvenance(providerID: .osm, sourceReference: nil, generatedAtUnixMs: 0)
        )
    }

    private func makeContentState(
        route r: NormalizedRoutePackage? = nil,
        progress: Double = 0,
        offRoute: Bool = false,
        rerouting: Bool = false,
        arrived: Bool = false,
        isImperial: Bool = false,
        now: Date = Date(timeIntervalSince1970: 1_700_000_000)
    ) -> RouteGuidanceActivityAttributes.ContentState? {
        LiveActivityMapper.contentState(
            route: r ?? route(),
            progressDistanceM: progress,
            offRoute: offRoute,
            rerouting: rerouting,
            arrived: arrived,
            isImperial: isImperial,
            now: now
        )
    }

    func test_contentState_skipsDepartWhenChoosingNext() {
        let s = makeContentState(progress: 0)
        XCTAssertEqual(s?.glyph, .left)            // m1 left at 200, not depart at 0
        XCTAssertEqual(s?.distanceToNextM ?? 0, 200, accuracy: 0.01)
    }

    func test_contentState_advancesPastFirstManeuver() {
        let s = makeContentState(progress: 250)
        XCTAssertEqual(s?.glyph, .right)           // m2 right at 600
        XCTAssertEqual(s?.distanceToNextM ?? 0, 350, accuracy: 0.01)
    }

    func test_contentState_distanceRemainingIsTotalMinusProgress() {
        let s = makeContentState(progress: 250)
        XCTAssertEqual(s?.distanceRemainingM ?? 0, 750, accuracy: 0.01)
    }

    func test_contentState_etaIsNowPlusProportionalSecondsRemaining() {
        // 1000 m / 600 s → 0.6 m/s. With 750 m remaining → 450 s ahead.
        let now = Date(timeIntervalSince1970: 1_700_000_000)
        let s = makeContentState(progress: 250, now: now)
        let eta = Date(timeIntervalSince1970: TimeInterval(Int64(s!.etaUnixMs) / 1000))
        XCTAssertEqual(eta.timeIntervalSince(now), 450, accuracy: 1.0)
    }

    func test_contentState_etaAdvancesAsProgressIncreases() {
        let now = Date(timeIntervalSince1970: 1_700_000_000)
        let s1 = makeContentState(progress: 100, now: now)!
        let s2 = makeContentState(progress: 500, now: now)!
        XCTAssertLessThan(s2.etaUnixMs, s1.etaUnixMs)
    }

    func test_contentState_compoundGlyphOnBackToBackUpcomingPair() {
        let r = route(maneuvers: [
            RouteManeuver(id: "m0", maneuverType: .depart,
                          location: .init(latitude: 0, longitude: 0),
                          distanceFromStartMeters: 0,
                          distanceToNextMeters: nil, instructionText: nil),
            RouteManeuver(id: "m1", maneuverType: .left,
                          location: .init(latitude: 0, longitude: 0),
                          distanceFromStartMeters: 200,
                          distanceToNextMeters: nil, instructionText: nil),
            RouteManeuver(id: "m2", maneuverType: .right,
                          location: .init(latitude: 0, longitude: 0),
                          distanceFromStartMeters: 220,  // 20 m gap → compound
                          distanceToNextMeters: nil, instructionText: nil),
            RouteManeuver(id: "m3", maneuverType: .arrive,
                          location: .init(latitude: 0, longitude: 0),
                          distanceFromStartMeters: 800,
                          distanceToNextMeters: nil, instructionText: nil),
        ])
        let s = makeContentState(route: r, progress: 100)
        XCTAssertEqual(s?.glyph, .leftThenRight)
        XCTAssertEqual(s?.distanceToNextM ?? 0, 100, accuracy: 0.01) // 200 - 100
    }

    func test_contentState_statusPrecedence_arrivedBeatsOffRoute() {
        let s = makeContentState(progress: 999, offRoute: true, arrived: true)
        XCTAssertEqual(s?.status, .arrived)
    }

    func test_contentState_statusOffRoute() {
        let s = makeContentState(progress: 100, offRoute: true)
        XCTAssertEqual(s?.status, .offRoute)
    }

    func test_contentState_statusRerouting() {
        let s = makeContentState(progress: 100, rerouting: true)
        XCTAssertEqual(s?.status, .rerouting)
    }

    func test_contentState_statusOnRouteDefault() {
        let s = makeContentState(progress: 100)
        XCTAssertEqual(s?.status, .onRoute)
    }

    func test_contentState_isImperialPropagated() {
        let s = makeContentState(progress: 100, isImperial: true)
        XCTAssertEqual(s?.isImperial, true)
    }

    func test_contentState_arrivedSetsArriveGlyphAndZeroDistance() {
        let s = makeContentState(progress: 1000, arrived: true)
        XCTAssertEqual(s?.glyph, .arrive)
        XCTAssertEqual(s?.distanceToNextM ?? -1, 0, accuracy: 0.01)
        XCTAssertEqual(s?.distanceRemainingM ?? -1, 0, accuracy: 0.01)
    }
}
