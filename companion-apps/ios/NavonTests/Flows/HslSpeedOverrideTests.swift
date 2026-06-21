import XCTest
@testable import Navon

/// Pure-function tests for the HSL cycling-speed ETA override.
///
/// planRoute() and replanRoute() live tests are in HslLiveRoutingTests.swift
/// — they require a running hsl-proxy server and are gated behind an opt-in flag.
@MainActor
final class HslSpeedOverrideTests: XCTestCase {

    func test_overrideDurationSeconds_helperRoundsToTotalDistanceOverSpeed() {
        // 2500 m / (18 kph / 3.6) = 2500 / 5.0 = 500 s
        let s = HslRoutingAdapter.overrideDurationSeconds(
            totalDistanceMeters: 2500,
            cyclingSpeedKph: 18,
            fallbackSeconds: 999
        )
        XCTAssertEqual(s, 500, "18 kph over 2.5 km should produce 500 s")
    }

    func test_overrideDurationSeconds_returnsFallbackForNonPositiveSpeed() {
        let s = HslRoutingAdapter.overrideDurationSeconds(
            totalDistanceMeters: 2500,
            cyclingSpeedKph: 0,
            fallbackSeconds: 600
        )
        XCTAssertEqual(s, 600, "non-positive speed must fall back instead of dividing by zero")
    }

    func test_overrideDurationSeconds_returnsFallbackForInfiniteSpeed() {
        let s = HslRoutingAdapter.overrideDurationSeconds(
            totalDistanceMeters: 2500,
            cyclingSpeedKph: .infinity,
            fallbackSeconds: 600
        )
        XCTAssertEqual(s, 600, "infinite speed must fall back")
    }
}
