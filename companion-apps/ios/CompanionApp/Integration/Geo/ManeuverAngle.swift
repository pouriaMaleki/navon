import Foundation

/// Shared geometry helpers used by the cue snapshot builder to compute
/// the actual turn angle at a maneuver point.

private let metersPerDegreeLat = 111_320.0
private let angleLookDistanceM = 10.0

func cumulativeDistances(_ geometry: [CoordinatePoint]) -> [Double] {
    guard geometry.count > 1 else { return geometry.isEmpty ? [] : [0] }
    var cum = [0.0]
    for i in 1..<geometry.count {
        cum.append(cum[i - 1] + haversineDistance(geometry[i - 1], geometry[i]))
    }
    return cum
}

func closestPointIndex(in geometry: [CoordinatePoint], to target: CoordinatePoint) -> Int? {
    guard !geometry.isEmpty else { return nil }
    var best = 0
    var bestDistSq = Double.infinity
    for (i, pt) in geometry.enumerated() {
        let dlat = pt.latitude - target.latitude
        let dlon = pt.longitude - target.longitude
        let distSq = dlat * dlat + dlon * dlon
        if distSq < bestDistSq {
            bestDistSq = distSq
            best = i
        }
    }
    return best
}

func maneuverAngleDegrees(_ geometry: [CoordinatePoint], _ cumulative: [Double], _ index: Int) -> Double {
    let maneuverPoint = geometry[index]
    let behind = walkAlongPolyline(geometry, cumulative, index, angleLookDistanceM, direction: "backward")
    let ahead = walkAlongPolyline(geometry, cumulative, index, angleLookDistanceM, direction: "forward")
    return turnDeltaDegrees(previous: behind, current: maneuverPoint, next: ahead)
}

func walkAlongPolyline(
    _ geometry: [CoordinatePoint],
    _ cumulative: [Double],
    _ startIndex: Int,
    _ distanceM: Double,
    direction: String
) -> CoordinatePoint {
    var remaining = distanceM
    var idx = startIndex
    while remaining > 1e-6, idx > 0, idx < geometry.count - 1 {
        let nextIdx = direction == "backward" ? idx - 1 : idx + 1
        guard nextIdx >= 0, nextIdx < geometry.count else { break }
        let segLen = direction == "backward"
            ? cumulative[idx] - cumulative[nextIdx]
            : cumulative[nextIdx] - cumulative[idx]
        if segLen <= 1e-9 { idx = nextIdx; continue }
        if remaining >= segLen {
            remaining -= segLen
            idx = nextIdx
        } else {
            let t = remaining / segLen
            let from = direction == "backward" ? geometry[idx] : geometry[idx]
            let to = direction == "backward" ? geometry[nextIdx] : geometry[nextIdx]
            return CoordinatePoint(
                latitude: from.latitude + (to.latitude - from.latitude) * t,
                longitude: from.longitude + (to.longitude - from.longitude) * t
            )
        }
    }
    return geometry[idx]
}

private func turnDeltaDegrees(previous: CoordinatePoint, current: CoordinatePoint, next: CoordinatePoint) -> Double {
    let incoming = bearingDegrees(from: previous, to: current)
    let outgoing = bearingDegrees(from: current, to: next)
    var delta = outgoing - incoming
    while delta <= -180 { delta += 360 }
    while delta > 180 { delta -= 360 }
    return delta
}

func bearingDegrees(from start: CoordinatePoint, to end: CoordinatePoint) -> Double {
    let latMeters = (end.latitude - start.latitude) * metersPerDegreeLat
    let meanLat = ((start.latitude + end.latitude) / 2) * (.pi / 180)
    let lonMeters = (end.longitude - start.longitude) * cos(meanLat) * metersPerDegreeLat
    return atan2(lonMeters, latMeters) * 180 / .pi
}

private func haversineDistance(_ a: CoordinatePoint, _ b: CoordinatePoint) -> Double {
    let dlat = (b.latitude - a.latitude) * metersPerDegreeLat
    let meanLat = ((a.latitude + b.latitude) / 2) * (.pi / 180)
    let dlon = (b.longitude - a.longitude) * cos(meanLat) * metersPerDegreeLat
    return sqrt(dlat * dlat + dlon * dlon)
}

// MARK: - Maneuver collapse

private let collapseDistanceM = 5.0
private let collapseAngleDeg = 30.0
private let maneuverLookDistM = 10.0

private func coordAtDistance(_ geometry: [CoordinatePoint], _ dist: Double, _ cumul: [Double]) -> CoordinatePoint {
    if dist <= 0 { return geometry[0] }
    let total = cumul[cumul.count - 1]
    if dist >= total { return geometry[geometry.count - 1] }
    for i in 1..<cumul.count {
        if cumul[i] >= dist {
            let segLen = cumul[i] - cumul[i - 1]
            let t = segLen > 1e-9 ? (dist - cumul[i - 1]) / segLen : 0
            return CoordinatePoint(
                latitude: geometry[i - 1].latitude + (geometry[i].latitude - geometry[i - 1].latitude) * t,
                longitude: geometry[i - 1].longitude + (geometry[i].longitude - geometry[i - 1].longitude) * t
            )
        }
    }
    return geometry[geometry.count - 1]
}

/// Collapse back-to-back maneuvers that are very close (<5m) when the net
/// direction change through them is >30°. Shared pedestrian path entries/exits
/// create multiple annotations but only the final real turn matters.
func collapseCloseManeuvers(_ maneuvers: [RouteManeuver], geometry: [CoordinatePoint]) -> [RouteManeuver] {
    if maneuvers.count < 2 { return maneuvers }
    if geometry.count < 2 { return maneuvers }

    let cumul = cumulativeDistances(geometry)
    let totalDist = cumul[cumul.count - 1]

    func netAngleDeg(firstDist: Double, lastDist: Double) -> Double {
        let approachFrom = coordAtDistance(geometry, max(0, firstDist - maneuverLookDistM), cumul)
        let approachPt = coordAtDistance(geometry, firstDist, cumul)
        let exitPt = coordAtDistance(geometry, lastDist, cumul)
        let exitTo = coordAtDistance(geometry, min(totalDist, lastDist + maneuverLookDistM), cumul)
        let inBearing = bearingDegrees(from: approachFrom, to: approachPt)
        let outBearing = bearingDegrees(from: exitPt, to: exitTo)
        var delta = outBearing - inBearing
        while delta <= -180 { delta += 360 }
        while delta > 180 { delta -= 360 }
        return abs(delta)
    }

    var result: [RouteManeuver] = []
    var i = 0
    while i < maneuvers.count {
        var j = i + 1
        while j < maneuvers.count &&
                maneuvers[j].distanceFromStartMeters - maneuvers[j - 1].distanceFromStartMeters < collapseDistanceM {
            j += 1
        }

        if j - i > 1 {
            let firstDist = maneuvers[i].distanceFromStartMeters
            let lastDist = maneuvers[j - 1].distanceFromStartMeters
            if netAngleDeg(firstDist: firstDist, lastDist: lastDist) > collapseAngleDeg {
                result.append(maneuvers[j - 1])
            } else {
                for k in i..<j { result.append(maneuvers[k]) }
            }
        } else {
            result.append(maneuvers[i])
        }
        i = j
    }

    return result
}
