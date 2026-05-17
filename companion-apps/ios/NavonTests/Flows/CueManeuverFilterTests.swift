import XCTest
@testable import Navon

/// Filter contract for `CueManeuverMapping.kind(for:)` — the single point
/// of truth that decides which `RouteManeuverType` values produce an audio
/// cue and which are silenced. Bugs covered:
///   - Bug 1: `.straight` must produce no cue (otherwise the engine fires
///     "Next turn in about X meters" / "Follow the route" for a non-turn).
///   - Bug 2: `.roundabout` / `.merge` / `.ramp` must map to first-class
///     kinds, not `.generic` (otherwise UI-mismatched generic phrasing).
///   - Bug 3: no `RouteManeuverType` may produce `.keepLeft` or
///     `.keepRight` (eliminated entirely from `ManeuverKind`; this test
///     is enforced by Swift's type system — included here as a
///     human-readable contract assertion).
final class CueManeuverFilterTests: XCTestCase {

    // MARK: - Bug 1: straight is not a turn

    func test_straightManeuver_producesNoCue() {
        XCTAssertNil(
            CueManeuverMapping.kind(for: .straight),
            ".straight must not produce an audio cue — it's not a turn the rider acts on"
        )
    }

    // MARK: - Bug 2: first-class roundabout / merge / ramp

    func test_roundaboutManeuver_producesRoundaboutKind() {
        XCTAssertEqual(CueManeuverMapping.kind(for: .roundabout), .roundabout)
    }

    func test_mergeManeuver_producesMergeKind() {
        XCTAssertEqual(CueManeuverMapping.kind(for: .merge), .merge)
    }

    func test_rampManeuver_producesRampKind() {
        XCTAssertEqual(CueManeuverMapping.kind(for: .ramp), .ramp)
    }

    // MARK: - Existing silence-by-design contract (regression guards)

    func test_slightLeft_mapsToSlightLeftKind() {
        XCTAssertEqual(CueManeuverMapping.kind(for: .slightLeft), .slightLeft)
    }

    func test_slightRight_mapsToSlightRightKind() {
        XCTAssertEqual(CueManeuverMapping.kind(for: .slightRight), .slightRight)
    }

    func test_depart_producesNoCue() {
        XCTAssertNil(CueManeuverMapping.kind(for: .depart))
    }

    func test_arrive_producesNoCue() {
        XCTAssertNil(CueManeuverMapping.kind(for: .arrive))
    }

    // MARK: - Real turns still fire

    func test_left_producesLeftKind() {
        XCTAssertEqual(CueManeuverMapping.kind(for: .left), .left)
    }

    func test_sharpLeft_producesLeftKind() {
        XCTAssertEqual(CueManeuverMapping.kind(for: .sharpLeft), .left)
    }

    func test_right_producesRightKind() {
        XCTAssertEqual(CueManeuverMapping.kind(for: .right), .right)
    }

    func test_sharpRight_producesRightKind() {
        XCTAssertEqual(CueManeuverMapping.kind(for: .sharpRight), .right)
    }

    func test_uturn_producesUturnKind() {
        XCTAssertEqual(CueManeuverMapping.kind(for: .uturn), .uturn)
    }

    // MARK: - Bug 3: no path produces keepLeft / keepRight
    //
    // Swift's type system enforces this — `.keepLeft` / `.keepRight` are
    // no longer ManeuverKind cases. Iterate every RouteManeuverType case
    // and assert the result is one of the kinds we still allow. If a
    // future routing-data type is added that maps to .keepLeft/.keepRight,
    // the enum has no such case and the build fails.

    func test_noManeuverTypeProducesUnreachableKind() {
        let allTypes: [RouteManeuverType] = [
            .depart, .straight, .slightLeft, .left, .sharpLeft,
            .slightRight, .right, .sharpRight, .uturn,
            .roundabout, .merge, .ramp, .arrive,
        ]
        let allowed: Set<ManeuverKind> = [
            .left, .right, .exitLeft, .exitRight,
            .uturn, .roundabout, .merge, .ramp, .generic,
        ]
        for type in allTypes {
            if let kind = CueManeuverMapping.kind(for: type) {
                XCTAssertTrue(
                    allowed.contains(kind),
                    "\(type) maps to \(kind), which is not in the allowed kinds set"
                )
            }
        }
    }
}
