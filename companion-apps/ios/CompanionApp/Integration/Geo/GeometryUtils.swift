import Foundation

/// Shared geometry helpers used across routing adapters.

/// Approximate metres per degree of latitude at the equator.
public let metersPerDegLat = 111_320.0

public func approximateDistanceMeters(from start: CoordinatePoint, to end: CoordinatePoint) -> Double {
    let latMeters = (end.latitude - start.latitude) * metersPerDegLat
    let meanLat = ((start.latitude + end.latitude) / 2) * .pi / 180.0
    let lonMeters = (end.longitude - start.longitude) * cos(meanLat) * metersPerDegLat
    return sqrt(latMeters * latMeters + lonMeters * lonMeters)
}

public func bearingDegrees(from start: CoordinatePoint, to end: CoordinatePoint) -> Double {
    let latMeters = (end.latitude - start.latitude) * metersPerDegLat
    let meanLat = ((start.latitude + end.latitude) / 2) * .pi / 180.0
    let lonScale = cos(meanLat) * metersPerDegLat
    let lonMeters = (end.longitude - start.longitude) * lonScale
    return atan2(lonMeters, latMeters) * 180.0 / .pi
}

public func cumulativeDistances(for geometry: [CoordinatePoint]) -> [Double] {
    var cumulative: [Double] = [0.0]
    for (start, end) in zip(geometry, geometry.dropFirst()) {
        cumulative.append(cumulative.last! + approximateDistanceMeters(from: start, to: end))
    }
    return cumulative
}

public func turnDeltaDegrees(previous: CoordinatePoint, current: CoordinatePoint, next: CoordinatePoint) -> Double {
    let incoming = bearingDegrees(from: previous, to: current)
    let outgoing = bearingDegrees(from: current, to: next)
    var delta = outgoing - incoming
    while delta <= -180.0 { delta += 360.0 }
    while delta > 180.0 { delta -= 360.0 }
    return delta
}

public func shiftPointByHeading(
    point: CoordinatePoint,
    headingDegrees: Double,
    distanceMeters: Double
) -> CoordinatePoint {
    let rad = headingDegrees * .pi / 180.0
    let northM = cos(rad) * distanceMeters
    let eastM = sin(rad) * distanceMeters
    let latitude = point.latitude + northM / metersPerDegLat
    let lonScale = metersPerDegLat * cos(point.latitude * .pi / 180.0)
    let longitude = lonScale == 0
        ? point.longitude
        : point.longitude + eastM / lonScale
    return CoordinatePoint(latitude: latitude, longitude: longitude)
}

public func headingBiasedOrigin(
    riderLocation: CoordinatePoint,
    rerouteContext: RerouteContext?,
    minSpeedMps: Double,
    forwardShiftM: Double,
    providerLabel: String
) -> CoordinatePoint {
    guard let heading = rerouteContext?.headingDegrees, heading.isFinite else {
        print("[reroute_heading] provider=\(providerLabel) reason=no_heading")
        return riderLocation
    }
    guard let speed = rerouteContext?.speedMps, speed.isFinite, speed >= minSpeedMps else {
        let speedLog = rerouteContext?.speedMps.map { String($0) } ?? "nil"
        print("[reroute_heading] provider=\(providerLabel) reason=low_speed speed=\(speedLog)")
        return riderLocation
    }
    let shifted = shiftPointByHeading(point: riderLocation, headingDegrees: heading, distanceMeters: forwardShiftM)
    if !shifted.latitude.isFinite || !shifted.longitude.isFinite ||
        (shifted.latitude == riderLocation.latitude && shifted.longitude == riderLocation.longitude) {
        print("[reroute_heading] provider=\(providerLabel) reason=shift_failed")
        return riderLocation
    }
    print("[reroute_heading] provider=\(providerLabel) reason=applied")
    return shifted
}
