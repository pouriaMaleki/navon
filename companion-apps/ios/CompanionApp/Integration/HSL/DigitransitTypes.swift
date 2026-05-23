import Foundation

extension HslRoutingAdapter {
    struct DigitransitGraphQLRequestBody: Encodable {
        var query: String
        var variables: Variables

        struct Variables: Encodable {
            var from: CoordinateVariable
            var to: CoordinateVariable
            var numItineraries: Int
            var transportModes: [TransportMode]
            var optimize: String
        }

        struct CoordinateVariable: Encodable {
            var lat: Double
            var lon: Double
        }

        struct TransportMode: Encodable {
            var mode: String
        }
    }

    struct DigitransitApiResponse: Decodable {
        var data: LiveDataContainer?
        var errors: [LiveError]?
    }

    struct LiveError: Decodable {
        var message: String
    }

    struct LiveDataContainer: Decodable {
        var plan: LivePlan?
    }

    struct LivePlan: Decodable {
        var itineraries: [LiveItinerary]
    }

    struct LiveItinerary: Decodable {
        var duration: Double
        var legs: [LiveLeg]
    }

    struct LiveLeg: Decodable {
        var mode: String?
        var distance: Double
        var from: LivePlace?
        var to: LivePlace?
        var legGeometry: LiveLegGeometry?
    }

    struct LivePlace: Decodable {
        var lat: Double
        var lon: Double
        var name: String?
    }

    struct LiveLegGeometry: Decodable {
        var points: String
    }

    struct DigitransitResponse {
        var data: DataContainer
    }

    struct DataContainer {
        var plan: DigitransitPlan
    }

    struct DigitransitPlan {
        var itineraries: [DigitransitItinerary]
    }

    struct DigitransitItinerary {
        var durationSeconds: Int
        var systemNotice: String
        var legs: [DigitransitLeg]
        var steps: [DigitransitStep]
        var startLabel: String
        var destinationLabel: String
    }

    struct DigitransitLeg {
        var mode: String
        var distanceMeters: Double
        var geometry: [CoordinatePoint]
    }

    struct DigitransitStep {
        var relativeDirection: String
        var location: CoordinatePoint
        var distanceFromStartMeters: Double
        var distanceToNextMeters: Double?
        var instruction: String
    }
}
