import XCTest
@testable import Navon

/// Backoff for repeat reroute attempts in a short window. User feedback:
/// when the rider keeps drifting off-route the planner spams reroute calls.
/// Hold the next auto-reroute by 5 s after the 3rd attempt in 30 s, by 10 s
/// at the 5th. Always expose a "Reroute now" override.
@MainActor
final class ReroutingBackoffTests: XCTestCase {

    private func freshApp() -> AppModel {
        let defaults = UserDefaults(suiteName: "rerouting-backoff-tests-\(UUID().uuidString)")!
        return AppModel(persistence: CompanionPersistence(defaults: defaults))
    }

    func test_firstAttempt_fires_immediately() {
        let vm = HomeViewModel(appModel: freshApp())
        XCTAssertEqual(vm.recordReroutingAttempt(now: 1_000), 0)
    }

    func test_secondAttempt_fires_immediately() {
        let vm = HomeViewModel(appModel: freshApp())
        _ = vm.recordReroutingAttempt(now: 1_000)
        XCTAssertEqual(vm.recordReroutingAttempt(now: 2_000), 0)
    }

    func test_thirdAttemptInWindow_delayedBy5s() {
        let vm = HomeViewModel(appModel: freshApp())
        _ = vm.recordReroutingAttempt(now: 1_000)
        _ = vm.recordReroutingAttempt(now: 2_000)
        XCTAssertEqual(vm.recordReroutingAttempt(now: 3_000), 5_000)
    }

    func test_fifthAttemptInWindow_escalatesTo10s() {
        let vm = HomeViewModel(appModel: freshApp())
        _ = vm.recordReroutingAttempt(now: 1_000)
        _ = vm.recordReroutingAttempt(now: 2_000)
        _ = vm.recordReroutingAttempt(now: 3_000)
        _ = vm.recordReroutingAttempt(now: 4_000)
        XCTAssertEqual(vm.recordReroutingAttempt(now: 5_000), 10_000)
    }

    func test_attemptsAgeOutAfter30s() {
        let vm = HomeViewModel(appModel: freshApp())
        for i in 0..<5 { _ = vm.recordReroutingAttempt(now: Double(i) * 1_000) }
        // 30 s after the last attempt, all five have aged out.
        XCTAssertEqual(vm.recordReroutingAttempt(now: 35_000), 0)
    }

    func test_isWaitingToReroute_trueDuringDelay() {
        let vm = HomeViewModel(appModel: freshApp())
        _ = vm.recordReroutingAttempt(now: 1_000)
        _ = vm.recordReroutingAttempt(now: 2_000)
        _ = vm.recordReroutingAttempt(now: 3_000) // → 5 s delay, ready at 8 s
        XCTAssertTrue(vm.isWaitingToReroute(now: 7_999))
        XCTAssertFalse(vm.isWaitingToReroute(now: 8_000))
    }

    func test_requestManualReroute_clearsTheDelay() {
        let vm = HomeViewModel(appModel: freshApp())
        _ = vm.recordReroutingAttempt(now: 1_000)
        _ = vm.recordReroutingAttempt(now: 2_000)
        _ = vm.recordReroutingAttempt(now: 3_000)
        XCTAssertTrue(vm.isWaitingToReroute(now: 4_000))
        vm.requestManualReroute()
        XCTAssertFalse(vm.isWaitingToReroute(now: 4_000))
        XCTAssertNil(vm.reroutingDelayedUntilMs)
    }
}
