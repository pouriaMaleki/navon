import Foundation

private let metersPerDegreeLat = 111_320.0
private let angleLookDistanceM = 10.0

func bearingDegrees(from start: CoordinatePoint, to end: CoordinatePoint) -> Double {
    let latMeters = (end.latitude - start.latitude) * metersPerDegreeLat
    let meanLat = ((start.latitude + end.latitude) / 2) * (.pi / 180)
    let lonMeters = (end.longitude - start.longitude) * cos(meanLat) * metersPerDegreeLat
    return atan2(lonMeters, latMeters) * 180 / .pi
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

func turnDeltaDegrees(previous: CoordinatePoint, current: CoordinatePoint, next: CoordinatePoint) -> Double {
    let incoming = bearingDegrees(from: previous, to: current)
    let outgoing = bearingDegrees(from: current, to: next)
    var delta = outgoing - incoming
    while delta <= -180 { delta += 360 }
    while delta > 180 { delta -= 360 }
    return delta
}
