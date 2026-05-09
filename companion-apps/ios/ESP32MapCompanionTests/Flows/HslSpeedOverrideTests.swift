import XCTest
@testable import ESP32MapCompanion

/// Mirrors the web `hslSpeed.test.ts`: the cycling-speed setting overrides the
/// HSL ETA so listed times match real-world riding rather than Digitransit's
/// conservative defaults.
///
/// Why existing tests didn't cover this: the iOS HSL adapter was only
/// tested at the BRouter level (`CyclingRoutingTests`); nothing exercised
/// the duration-override pipeline.
@MainActor
final class HslSpeedOverrideTests: XCTestCase {

    private let origin = CoordinatePoint(latitude: 60.1699, longitude: 24.9384)
    private let destination = CoordinatePoint(latitude: 60.1921, longitude: 24.9458)

    private func adapter(cyclingSpeedKph: Double) -> HslRoutingAdapter {
        HslRoutingAdapter(settingsProvider: {
            CompanionSettings(
                preferLiveHslRouting: false,
                hslSubscriptionKey: "",
                hslEndpointURL: CompanionSettings.defaults.hslEndpointURL,
                cyclingSpeedKph: cyclingSpeedKph,
                speedUnit: .kph,
                ridingCameraDistanceM: nil
            )
        })
    }

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

    func test_planRoute_appliesCyclingSpeedToEachAlternative() async throws {
        let adapter = adapter(cyclingSpeedKph: 18)
        let preview = try await adapter.planRoute(
            RoutePlanRequest(origin: origin, destination: destination, providerID: .hsl)
        )
        XCTAssertGreaterThan(preview.alternatives.count, 0)
        for alt in preview.alternatives {
            let distance = alt.normalizedPackage.summary.totalDistanceMeters
            let expected = HslRoutingAdapter.overrideDurationSeconds(
                totalDistanceMeters: distance,
                cyclingSpeedKph: 18,
                fallbackSeconds: alt.normalizedPackage.summary.estimatedDurationSeconds
            )
            XCTAssertEqual(
                alt.normalizedPackage.summary.estimatedDurationSeconds, expected,
                "summary ETA should reflect the cycling-speed override"
            )
            XCTAssertEqual(
                alt.durationSeconds, expected,
                "RouteAlternative.durationSeconds should match the override"
            )
        }
    }

    func test_planRoute_higherSpeedYieldsLowerEtaThanLowerSpeed() async throws {
        let slow = adapter(cyclingSpeedKph: 12)
        let fast = adapter(cyclingSpeedKph: 25)
        let req = RoutePlanRequest(origin: origin, destination: destination, providerID: .hsl)
        let slowPreview = try await slow.planRoute(req)
        let fastPreview = try await fast.planRoute(req)
        let slowSec = slowPreview.alternatives[0].normalizedPackage.summary.estimatedDurationSeconds
        let fastSec = fastPreview.alternatives[0].normalizedPackage.summary.estimatedDurationSeconds
        XCTAssertLessThan(fastSec, slowSec)
    }

    func test_replanRoute_appliesHeadingBiasWhenSpeedIsHigh() async throws {
        let a = adapter(cyclingSpeedKph: 18)
        let session = ActiveRouteSession(
            routeIdentifier: "r1",
            routeRevision: 1,
            destinationLabel: "Dest",
            destinationCoordinate: destination,
            providerID: .hsl,
            sourceMode: .hsl
        )
        let preview = try await a.replanRoute(
            using: session,
            riderLocation: origin,
            rerouteContext: RerouteContext(headingDegrees: 90, speedMps: 4.0)
        )
        XCTAssertNotEqual(preview.alternatives.first?.normalizedPackage.geometry.first, origin)
    }

    func test_replanRoute_keepsLegacyOriginWhenSpeedIsLow() async throws {
        let a = adapter(cyclingSpeedKph: 18)
        let session = ActiveRouteSession(
            routeIdentifier: "r1",
            routeRevision: 1,
            destinationLabel: "Dest",
            destinationCoordinate: destination,
            providerID: .hsl,
            sourceMode: .hsl
        )
        let preview = try await a.replanRoute(
            using: session,
            riderLocation: origin,
            rerouteContext: RerouteContext(headingDegrees: 90, speedMps: 0.5)
        )
        XCTAssertEqual(preview.alternatives.first?.normalizedPackage.geometry.first, origin)
    }
}
