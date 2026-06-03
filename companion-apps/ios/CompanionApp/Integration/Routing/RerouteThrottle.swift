import Foundation

@MainActor
final class RerouteThrottle {
    private(set) var attemptTimestampsMs: [Double] = []
    private(set) var delayedUntilMs: Double?

    static let backoffWindowMs: Double = 30_000
    static let throttleAtAttempts: Int = 3
    static let escalateAtAttempts: Int = 5
    static let backoffDelayMs: Double = 5_000
    static let backoffLongDelayMs: Double = 10_000

    @discardableResult
    func recordAttempt(now: Double) -> Double {
        attemptTimestampsMs.removeAll { now - $0 >= Self.backoffWindowMs }
        attemptTimestampsMs.append(now)
        let count = attemptTimestampsMs.count
        var delayMs: Double = 0
        if count >= Self.escalateAtAttempts {
            delayMs = Self.backoffLongDelayMs
        } else if count >= Self.throttleAtAttempts {
            delayMs = Self.backoffDelayMs
        }
        delayedUntilMs = delayMs > 0 ? now + delayMs : nil
        return delayMs
    }

    func isDelayed(now: Double) -> Bool {
        guard let until = delayedUntilMs else { return false }
        return now < until
    }

    func requestManualOverride() {
        delayedUntilMs = nil
    }

    func markDispatched() {
        delayedUntilMs = nil
    }

    func reset() {
        attemptTimestampsMs.removeAll()
        delayedUntilMs = nil
    }
}
