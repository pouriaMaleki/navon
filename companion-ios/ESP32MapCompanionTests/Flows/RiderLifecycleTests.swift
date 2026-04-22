import XCTest
@testable import ESP32MapCompanion

/// L2 rider-lifecycle replay — drives a FakeLocationService with the
/// helsinki-gravel fixture (plan flow #63).
@MainActor
final class RiderLifecycleTests: XCTestCase {

    func test_fixtureLoads() {
        let samples = HelsinkiGravelFixture.loadStream()
        XCTAssertGreaterThan(samples.count, 100)
        XCTAssertEqual(samples.first?.timeOffsetMs, 0)
    }

    func test_fakeLocationServicePublishesEachFix() async {
        let service = FakeLocationService()
        service.start()
        service.emitFix(latitude: 60.17, longitude: 24.94)
        XCTAssertEqual(service.currentLocation?.latitude, 60.17)
        service.emitFix(latitude: 60.18, longitude: 24.95)
        XCTAssertEqual(service.currentLocation?.latitude, 60.18)
    }

    func test_fullRidePlaybackReachesFinalFix() async {
        let samples = HelsinkiGravelFixture.loadStream()
        let service = FakeLocationService()
        service.start()
        for sample in samples {
            service.emitFix(latitude: sample.latitude, longitude: sample.longitude)
        }
        guard let last = samples.last else { return XCTFail("fixture empty") }
        XCTAssertEqual(service.currentLocation?.latitude, last.latitude, accuracy: 1e-9)
        XCTAssertEqual(service.currentLocation?.longitude, last.longitude, accuracy: 1e-9)
    }
}
