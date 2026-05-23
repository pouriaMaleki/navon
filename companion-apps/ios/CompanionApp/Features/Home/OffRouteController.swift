import Foundation

/// Off-route hysteresis and reroute backoff constants shared with the web
/// companion's GuidanceStore. Extracted for single-source-of-truth reference.

enum OffRouteController {
    static let enterDistanceM: Double = 35
    static let exitDistanceM: Double = 22
    static let requestDelayMs: Double = 2000

    static let backoffWindowMs: Double = 30_000
    static let throttleAtAttempts: Int = 3
    static let escalateAtAttempts: Int = 5
    static let backoffDelayMs: Double = 5_000
    static let backoffLongDelayMs: Double = 10_000
}
