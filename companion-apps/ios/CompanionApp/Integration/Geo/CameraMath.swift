import Foundation
import MapKit

enum CameraMath {
    static let defaultRidingCameraDistanceM: Double = 1200
    static let minRidingCameraDistanceM: Double = 250
    static let maxRidingCameraDistanceM: Double = 8000
    static let ridingZoomStepFactor: Double = 1.5
    static let anchorOffsetMetersPerDistance: Double = 0.075

    static func fittedRouteRegion(
        minLat: Double, maxLat: Double,
        minLon: Double, maxLon: Double
    ) -> MKCoordinateRegion {
        let latDelta = max((maxLat - minLat) * 4.0, 0.022)
        let lonDelta = max((maxLon - minLon) * 2.0, 0.014)
        let bboxCenterLat = (minLat + maxLat) / 2.0
        let shiftedCenterLat = bboxCenterLat - 0.22 * latDelta
        return MKCoordinateRegion(
            center: CLLocationCoordinate2D(
                latitude: shiftedCenterLat,
                longitude: (minLon + maxLon) / 2.0
            ),
            span: MKCoordinateSpan(latitudeDelta: latDelta, longitudeDelta: lonDelta)
        )
    }

    static func cameraCenterCoordinate(
        rider: CoordinatePoint,
        headingDegrees: Double,
        cameraDistanceM: Double
    ) -> CoordinatePoint {
        let anchorOffsetMeters = anchorOffsetMetersPerDistance * cameraDistanceM
        let metersPerDegLat = 111_320.0
        let headingRad = headingDegrees * .pi / 180.0
        let dNorth = cos(headingRad) * anchorOffsetMeters
        let dEast = sin(headingRad) * anchorOffsetMeters
        let cosLat = cos(rider.latitude * .pi / 180.0)
        return CoordinatePoint(
            latitude: rider.latitude + dNorth / metersPerDegLat,
            longitude: rider.longitude + dEast / (metersPerDegLat * cosLat)
        )
    }
}
