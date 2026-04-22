import Foundation

/// Small ring buffer of recent GPS fixes that derives a smoothed travel
/// heading. Matches the web + runtime-core contract (spec line 110,
/// authoritative): when the rider is moving, the camera rotates to the
/// GPS-derived direction — this overrides the route-segment bearing.
/// Returns nil while stationary / no usable trail.
@MainActor
final class HeadingTrail {
    private struct Fix {
        let point: CoordinatePoint
        let timestampMs: Int64
    }

    private var fixes: [Fix] = []
    private var smoothedDegrees: Double?

    let maxAgeMs: Int64
    let maxFixes: Int
    let minDisplacementM: Double
    let smoothingAlpha: Double

    init(maxAgeMs: Int64, maxFixes: Int, minDisplacementM: Double, smoothingAlpha: Double) {
        self.maxAgeMs = maxAgeMs
        self.maxFixes = maxFixes
        self.minDisplacementM = minDisplacementM
        self.smoothingAlpha = smoothingAlpha
    }

    func recordFix(_ point: CoordinatePoint, timestampMs: Int64) {
        evictOld(nowMs: timestampMs)
        fixes.append(Fix(point: point, timestampMs: timestampMs))
        if fixes.count > maxFixes { fixes.removeFirst() }
        guard let raw = computeRawHeading() else { return }
        if let prev = smoothedDegrees {
            let delta = shortestSignedDelta(from: prev, to: raw)
            smoothedDegrees = normalize360(prev + delta * smoothingAlpha)
        } else {
            smoothedDegrees = raw
        }
    }

    var travelHeadingDegrees: Double? { smoothedDegrees }

    func reset() {
        fixes.removeAll()
        smoothedDegrees = nil
    }

    private func evictOld(nowMs: Int64) {
        let cutoff = nowMs - maxAgeMs
        while let first = fixes.first, first.timestampMs < cutoff { fixes.removeFirst() }
        if fixes.isEmpty { smoothedDegrees = nil }
    }

    private func computeRawHeading() -> Double? {
        guard fixes.count >= 2 else { return nil }
        let first = fixes[0].point
        let last = fixes[fixes.count - 1].point
        let metersPerDegLat = 111_320.0
        let meanLat = ((first.latitude + last.latitude) / 2.0) * .pi / 180.0
        let dNorth: Double = (last.latitude - first.latitude) * metersPerDegLat
        let dEast: Double = (last.longitude - first.longitude) * cos(meanLat) * metersPerDegLat
        let displacement: Double = (dNorth * dNorth + dEast * dEast).squareRoot()
        if displacement < minDisplacementM { return nil }
        return normalize360(atan2(dEast, dNorth) * 180.0 / .pi)
    }
}

private func normalize360(_ deg: Double) -> Double {
    let r = deg.truncatingRemainder(dividingBy: 360.0)
    return r < 0 ? r + 360.0 : r
}

private func shortestSignedDelta(from a: Double, to b: Double) -> Double {
    var d = (b - a + 540.0).truncatingRemainder(dividingBy: 360.0) - 180.0
    if d == -180.0 { d = 180.0 }
    return d
}
