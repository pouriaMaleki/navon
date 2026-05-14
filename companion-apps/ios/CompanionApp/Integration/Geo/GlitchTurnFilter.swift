import Foundation

private let glitchClusterMaxGapM: Double = 10
private let glitchAngleThresholdDeg: Double = 10
private let glitchAngleLookDistanceM: Double = 10

/// Remove clusters of consecutive maneuvers that are map glitches:
/// >=2 maneuvers each within 10m of the previous, where the net path
/// direction change from before the cluster to after it is < 10 degrees.
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

        // Extend cluster forward while consecutive gaps <= threshold
        var clusterEnd = i + 1
        while clusterEnd + 1 < result.count {
            let nextGap = result[clusterEnd + 1].distanceFromStartMeters - result[clusterEnd].distanceFromStartMeters
            guard nextGap <= glitchClusterMaxGapM else { break }
            clusterEnd += 1
        }

        let clusterSize = clusterEnd - i + 1
        guard clusterSize >= 2 else { i = clusterEnd + 1; continue }

        // Compute net direction change before -> after the cluster
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
                continue // re-evaluate this position after removal
            }
        }
        i = clusterEnd + 1
    }
    return result
}
