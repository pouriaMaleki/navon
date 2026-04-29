import Foundation

/// Lock-screen Live Activity port. Tests inject a fake; production uses
/// `ActivityKitLiveActivityService` (added in `LiveActivityService.swift`).
///
/// Spec line 144: surface route status and the next turn on the lock screen
/// while routing is active and `liveActivityEnabled` is on.
struct RoutingLiveActivityContent: Equatable {
    var routeIdentifier: String
    var destinationLabel: String
    var nextInstruction: String
    var etaMinutes: Int
}

protocol LiveActivityPort: AnyObject {
    func start(_ content: RoutingLiveActivityContent)
    func update(_ content: RoutingLiveActivityContent)
    func end()
    /// End every previously-started routing activity, including ones
    /// orphaned from a prior app process. Called once at app launch to
    /// clean up after a force-quit while routing — by default
    /// ActivityKit keeps activities alive across app deaths.
    func endAllOutstanding()
}

/// No-op port. Used when the toggle is off, when `ActivityKit` is
/// unavailable (iOS 16.1 ships it; we deploy iOS 17+ so fine), and as a
/// test fallback to keep XCTest tests off `ActivityKit`.
final class NoopLiveActivityPort: LiveActivityPort {
    func start(_ content: RoutingLiveActivityContent) {}
    func update(_ content: RoutingLiveActivityContent) {}
    func end() {}
    func endAllOutstanding() {}
}

/// Spy used by tests to assert the wiring layer calls the port correctly.
final class SpyLiveActivityPort: LiveActivityPort {
    private(set) var startedWith: RoutingLiveActivityContent?
    private(set) var updates: [RoutingLiveActivityContent] = []
    private(set) var endedCount: Int = 0
    private(set) var endAllOutstandingCount: Int = 0

    func start(_ content: RoutingLiveActivityContent) { startedWith = content }
    func update(_ content: RoutingLiveActivityContent) { updates.append(content) }
    func end() { endedCount += 1 }
    func endAllOutstanding() { endAllOutstandingCount += 1 }
}
