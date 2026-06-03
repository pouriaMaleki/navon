import Foundation

private let metersPerDegreeLat = 111_320.0

enum PolylineGeo {

    static func formatDistance(_ meters: Double) -> String {
        if meters >= 1000 { return String(format: "%.1f km", meters / 1000) }
        return "\(Int(meters.rounded())) m"
    }

    static func polylineLengthMeters(_ points: [CoordinatePoint]) -> Double {
        guard points.count >= 2 else { return 0 }
        var total = 0.0
        for i in 0..<(points.count - 1) {
            let a = points[i]
            let b = points[i + 1]
            let meanLat = (a.latitude + b.latitude) / 2.0 * .pi / 180.0
            let dN = (b.latitude - a.latitude) * metersPerDegreeLat
            let dE = (b.longitude - a.longitude) * cos(meanLat) * metersPerDegreeLat
            total += (dN * dN + dE * dE).squareRoot()
        }
        return total
    }

    static func projectProgress(onto polyline: [CoordinatePoint], rider: CoordinatePoint) -> Double {
        guard polyline.count >= 2 else { return 0.0 }
        var bestDistSq = Double.infinity
        var bestProgress = 0.0
        var traversed = 0.0
        for i in 0..<(polyline.count - 1) {
            let a = polyline[i]
            let b = polyline[i + 1]
            let meanLat = ((a.latitude + rider.latitude) / 2.0) * .pi / 180.0
            let cosLat = cos(meanLat)
            let endX = (b.longitude - a.longitude) * cosLat * metersPerDegreeLat
            let endY = (b.latitude - a.latitude) * metersPerDegreeLat
            let riderX = (rider.longitude - a.longitude) * cosLat * metersPerDegreeLat
            let riderY = (rider.latitude - a.latitude) * metersPerDegreeLat
            let segLenSq = endX * endX + endY * endY
            guard segLenSq > 1e-12 else { continue }
            let t = max(0.0, min(1.0, (riderX * endX + riderY * endY) / segLenSq))
            let projX = t * endX
            let projY = t * endY
            let dx = riderX - projX
            let dy = riderY - projY
            let distSq = dx * dx + dy * dy
            if distSq < bestDistSq {
                bestDistSq = distSq
                let segLen = sqrt(segLenSq)
                bestProgress = traversed + segLen * t
            }
            traversed += sqrt(segLenSq)
        }
        return bestProgress
    }

    static func projectProgressWithDistance(
        onto polyline: [CoordinatePoint],
        rider: CoordinatePoint
    ) -> (progress: Double, distanceToRouteM: Double) {
        guard polyline.count >= 2 else { return (0, .infinity) }
        var bestDistSq = Double.infinity
        var bestProgress = 0.0
        var traversed = 0.0
        for i in 0..<(polyline.count - 1) {
            let a = polyline[i]
            let b = polyline[i + 1]
            let meanLat = ((a.latitude + rider.latitude) / 2.0) * .pi / 180.0
            let cosLat = cos(meanLat)
            let endX = (b.longitude - a.longitude) * cosLat * metersPerDegreeLat
            let endY = (b.latitude - a.latitude) * metersPerDegreeLat
            let riderX = (rider.longitude - a.longitude) * cosLat * metersPerDegreeLat
            let riderY = (rider.latitude - a.latitude) * metersPerDegreeLat
            let segLenSq = endX * endX + endY * endY
            guard segLenSq > 1e-12 else { continue }
            let t = max(0.0, min(1.0, (riderX * endX + riderY * endY) / segLenSq))
            let projX = t * endX
            let projY = t * endY
            let dx = riderX - projX
            let dy = riderY - projY
            let distSq = dx * dx + dy * dy
            if distSq < bestDistSq {
                bestDistSq = distSq
                let segLen = sqrt(segLenSq)
                bestProgress = traversed + segLen * t
            }
            traversed += sqrt(segLenSq)
        }
        return (bestProgress, sqrt(bestDistSq))
    }

    static func straightLineMeters(_ a: CoordinatePoint, _ b: CoordinatePoint) -> Double {
        let dN = (b.latitude - a.latitude) * metersPerDegreeLat
        let meanLat = ((a.latitude + b.latitude) / 2.0) * .pi / 180.0
        let dE = (b.longitude - a.longitude) * cos(meanLat) * metersPerDegreeLat
        return (dN * dN + dE * dE).squareRoot()
    }

    static func splitPolylineAtDistance(
        _ polyline: [CoordinatePoint],
        distance: Double
    ) -> (completed: [CoordinatePoint], remaining: [CoordinatePoint]) {
        guard polyline.count >= 2, distance > 0 else { return ([], polyline) }
        var traversed = 0.0
        var completed: [CoordinatePoint] = [polyline[0]]
        for i in 0..<(polyline.count - 1) {
            let a = polyline[i]
            let b = polyline[i + 1]
            let meanLat = ((a.latitude + b.latitude) / 2.0) * .pi / 180.0
            let dN = (b.latitude - a.latitude) * metersPerDegreeLat
            let dE = (b.longitude - a.longitude) * cos(meanLat) * metersPerDegreeLat
            let segLen = (dN * dN + dE * dE).squareRoot()
            if traversed + segLen >= distance {
                let t = segLen <= 0 ? 0 : (distance - traversed) / segLen
                let split = CoordinatePoint(
                    latitude: a.latitude + (b.latitude - a.latitude) * t,
                    longitude: a.longitude + (b.longitude - a.longitude) * t
                )
                completed.append(split)
                var remaining: [CoordinatePoint] = [split]
                remaining.append(contentsOf: polyline[(i + 1)...])
                return (completed, remaining)
            }
            completed.append(b)
            traversed += segLen
        }
        return (polyline, [polyline.last!])
    }
}
