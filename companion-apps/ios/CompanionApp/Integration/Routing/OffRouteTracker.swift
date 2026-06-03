import Foundation

/// Off-route hysteresis, progress tracking, arrival detection, and auto-reroute scheduling.
@MainActor
final class OffRouteTracker: ObservableObject {
    static let offRouteEnterDistanceM: Double = 35
    static let offRouteExitDistanceM: Double = 22
    static let rerouteRequestDelayMs: Double = 2000
    static let arrivalRadiusM: Double = 25

    private let rerouteThrottle: RerouteThrottle
    private var offRouteDurationMs: Double = 0
    private var lastAdvanceTimestampMs: Int64 = -1
    private var lastProgressRouteId: String? = nil

    var onDiagnosticsEvent: ((RoutingDiagEventData) -> Void)?
    var onRerouteNeeded: ((CoordinatePoint, RerouteContext?) async -> Void)?
    var onArrivalDetected: (() -> Void)?
    var onProgressTick: (() -> Void)?

    @Published var offRouteDistanceM: Double = 0
    @Published var offRoute: Bool = false
    @Published var rerouteRequested: Bool = false
    @Published var progressDistanceM: Double = 0
    @Published var reroutingAttemptTimestampsMs: [Double] = []
    @Published var reroutingDelayedUntilMs: Double?

    private(set) var autoReroutePending = false
    private(set) var pendingAutoRerouteTask: Task<Void, Never>?
    private(set) var routeTotalDistanceM: Double = 0

    init(rerouteThrottle: RerouteThrottle) {
        self.rerouteThrottle = rerouteThrottle
    }

    func recordReroutingAttempt(now: Double) -> Double {
        let delayMs = rerouteThrottle.recordAttempt(now: now)
        reroutingAttemptTimestampsMs = rerouteThrottle.attemptTimestampsMs
        reroutingDelayedUntilMs = rerouteThrottle.delayedUntilMs
        return delayMs
    }

    func isWaitingToReroute(now: Double) -> Bool {
        rerouteThrottle.isDelayed(now: now)
    }

    func requestManualReroute() {
        rerouteThrottle.requestManualOverride()
        reroutingDelayedUntilMs = nil
    }

    func markAutoRerouteDispatched() {
        rerouteRequested = false
        offRouteDurationMs = 0
        autoReroutePending = false
        rerouteThrottle.markDispatched()
        reroutingDelayedUntilMs = nil
    }

    func reset() {
        offRoute = false
        offRouteDistanceM = 0
        offRouteDurationMs = 0
        rerouteRequested = false
        rerouteThrottle.reset()
        reroutingAttemptTimestampsMs = []
        reroutingDelayedUntilMs = nil
        autoReroutePending = false
        pendingAutoRerouteTask?.cancel()
        pendingAutoRerouteTask = nil
        progressDistanceM = 0
        routeTotalDistanceM = 0
        lastProgressRouteId = nil
        lastAdvanceTimestampMs = -1
    }

    func setRouteTotalDistance(_ distance: Double) {
        routeTotalDistanceM = distance
    }

    func resetProgress() {
        progressDistanceM = 0
        routeTotalDistanceM = 0
        lastProgressRouteId = nil
        lastAdvanceTimestampMs = -1
    }

    func cancelPendingReroute() {
        pendingAutoRerouteTask?.cancel()
        pendingAutoRerouteTask = nil
        autoReroutePending = false
    }

    func advanceProgress(
        rider: CoordinatePoint,
        nowMs: Int64,
        geometry: [CoordinatePoint],
        routeKey: String?,
        travelHeadingDegrees: Double?,
        speedMps: Double?
    ) {
        guard geometry.count >= 2 else { return }
        if routeKey != lastProgressRouteId {
            lastProgressRouteId = routeKey
            progressDistanceM = 0
            routeTotalDistanceM = PolylineGeo.polylineLengthMeters(geometry)
            // Keep `lastAdvanceTimestampMs` — it tracks wall-clock dt between
            // fixes, which is independent of which route we're projecting onto.
            // Resetting it here breaks sustained off-route detection across
            // reroutes: after a reroute, the off-route accumulator would lose
            // a full dt sample and need another full `rerouteRequestDelayMs`
            // window from scratch before a follow-up reroute could fire.
        }
        let projection = PolylineGeo.projectProgressWithDistance(onto: geometry, rider: rider)
        let bounded = min(routeTotalDistanceM > 0 ? routeTotalDistanceM : projection.progress, projection.progress)
        progressDistanceM = max(progressDistanceM, bounded)
        offRouteDistanceM = projection.distanceToRouteM

        let dt = lastAdvanceTimestampMs >= 0 ? Double(nowMs - lastAdvanceTimestampMs) : 0
        lastAdvanceTimestampMs = nowMs

        // Off-route latch with hysteresis (35 m enter, 22 m exit) — same
        // bands as runtime-core / companion-web.
        let wasOffRoute = offRoute
        let prevRerouteRequested = rerouteRequested
        if offRoute {
            if projection.distanceToRouteM <= Self.offRouteExitDistanceM {
                offRoute = false
            }
        } else if projection.distanceToRouteM >= Self.offRouteEnterDistanceM {
            offRoute = true
            onDiagnosticsEvent?(.offRouteDetected(distanceM: projection.distanceToRouteM))
        }
        if offRoute {
            offRouteDurationMs = wasOffRoute ? offRouteDurationMs + dt : dt
            if offRouteDurationMs >= Self.rerouteRequestDelayMs {
                rerouteRequested = true
            }
        } else {
            offRouteDurationMs = 0
            rerouteRequested = false
            // Returning to the corridor re-arms auto-reroute for the
            // next off-route episode. Without this, only the first
            // departure of a session would ever re-fetch a route.
            autoReroutePending = false
        }

        // Rising edge of `rerouteRequested` (false → true) is the only
        // moment we kick the routing provider.
        if rerouteRequested && !prevRerouteRequested && !autoReroutePending {
            autoReroutePending = true
            onDiagnosticsEvent?(.rerouteRequested)
            let now = Date().timeIntervalSince1970 * 1_000
            let delayMs = recordReroutingAttempt(now: now)
            let onRerouteNeeded = self.onRerouteNeeded
            pendingAutoRerouteTask = Task { @MainActor [weak self] in
                guard let self else { return }
                if delayMs > 0 {
                    while let until = self.reroutingDelayedUntilMs,
                          Date().timeIntervalSince1970 * 1_000 < until {
                        try? await Task.sleep(nanoseconds: 250_000_000)
                        if Task.isCancelled { return }
                    }
                }
                self.markAutoRerouteDispatched()
                await onRerouteNeeded?(rider, RerouteContext(
                    headingDegrees: travelHeadingDegrees,
                    speedMps: speedMps
                ))
            }
        }

        // Spec: when the rider arrives at the destination, end routing.
        if let last = geometry.last,
           PolylineGeo.straightLineMeters(last, rider) <= Self.arrivalRadiusM {
            onArrivalDetected?()
        }

        onProgressTick?()
    }
}
