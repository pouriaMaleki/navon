import Foundation

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

private func haversineDistance(_ a: CoordinatePoint, _ b: CoordinatePoint) -> Double {
    let metersPerDegreeLat = 111_320.0
    let dlat = (b.latitude - a.latitude) * metersPerDegreeLat
    let meanLat = ((a.latitude + b.latitude) / 2) * (.pi / 180)
    let dlon = (b.longitude - a.longitude) * cos(meanLat) * metersPerDegreeLat
    return sqrt(dlat * dlat + dlon * dlon)
}

private let glitchClusterMaxGapM: Double = 10
private let glitchAngleThresholdDeg: Double = 10
private let glitchAngleLookDistanceM: Double = 10

/// Remove groups of consecutive maneuvers that are map-data glitches:
/// ≥2 maneuvers within 10 m of each other where the net direction change
/// through the cluster is < 10°.
func filterGlitchClusters(_ maneuvers: [RouteManeuver], geometry: [CoordinatePoint]) -> [RouteManeuver] {
    guard maneuvers.count >= 2, geometry.count >= 2 else { return maneuvers }

    let cumDist = cumulativeDistances(geometry)
    guard !cumDist.isEmpty else { return maneuvers }

    var result = maneuvers
    var i = 0
    while i < result.count {
        guard i + 1 < result.count else { break }
        let gap = result[i + 1].distanceFromStartMeters - result[i].distanceFromStartMeters
        if gap > glitchClusterMaxGapM { i += 1; continue }

        var clusterEnd = i + 1
        while clusterEnd + 1 < result.count {
            let nextGap = result[clusterEnd + 1].distanceFromStartMeters - result[clusterEnd].distanceFromStartMeters
            guard nextGap <= glitchClusterMaxGapM else { break }
            clusterEnd += 1
        }

        let clusterSize = clusterEnd - i + 1
        guard clusterSize >= 2 else { i = clusterEnd + 1; continue }

        if let firstIdx = closestPointIndex(in: geometry, to: result[i].location),
           let lastIdx = closestPointIndex(in: geometry, to: result[clusterEnd].location),
           firstIdx >= 0, firstIdx < geometry.count,
           lastIdx >= 0, lastIdx < geometry.count {

            let entryApproach = walkAlongPolyline(geometry, cumDist, firstIdx, glitchAngleLookDistanceM, direction: "backward")
            let exitDepart = walkAlongPolyline(geometry, cumDist, lastIdx, glitchAngleLookDistanceM, direction: "forward")
            let entryBearing = bearingDegrees(from: entryApproach, to: geometry[firstIdx])
            let exitBearing = bearingDegrees(from: geometry[lastIdx], to: exitDepart)
            var delta = abs(exitBearing - entryBearing)
            if delta > 180 { delta = 360 - delta }

            if delta < glitchAngleThresholdDeg {
                result.removeSubrange(i...clusterEnd)
                continue
            }
        }
        i = clusterEnd + 1
    }
    return result
}

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

/// Collapse back-to-back maneuvers that are very close (<5 m) when the net
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
