// Distance formatting helpers — bridges raw meters to the
// (number, unit) tuple consumed by cue ICU templates and produces
// ready-to-render UI labels via `units.distance.*` keys.

import Foundation

enum DistanceFormatter {
    private static let ftPerM: Double = 3.280839895
    private static let miPerM: Double = 0.0006213712
    private static let kmThresholdM: Double = 1000
    private static let miThresholdM: Double = 1609

    static func roundTo10(_ n: Double) -> Int {
        Int((n / 10.0).rounded()) * 10
    }

    /// Cue-engine helper: produce the ICU placeholder bundle for a
    /// distance voice cue. In metric mode emits `(50, "meters")`; in
    /// imperial mode converts to feet and rounds.
    static func cueValues(meters: Double, mode: DistanceMode) -> [String: MessageValue] {
        switch mode {
        case .imperial:
            return [
                "distance": .number(Double(roundTo10(meters * ftPerM))),
                "distanceUnit": .string("feet"),
            ]
        case .metric:
            if meters >= kmThresholdM {
                let km = (meters / 100.0).rounded() / 10.0 // one decimal
                return [
                    "distance": .number(km),
                    "distanceUnit": .string("kilometers"),
                ]
            }
            return [
                "distance": .number(Double(roundTo10(meters))),
                "distanceUnit": .string("meters"),
            ]
        }
    }

    /// UI helper: produce a "8.6 km" / "5.4 mi" label for arbitrary
    /// distances, choosing meters/feet for short ones and km/mi above
    /// ~1 unit.
    static func label(meters: Double, mode: DistanceMode) -> String {
        switch mode {
        case .imperial:
            if meters >= miThresholdM {
                let miles = meters * miPerM
                return T.string("units.distance.mi", ["distance": .number((miles * 10).rounded() / 10)])
            }
            return T.string("units.distance.ft", ["distance": .number(Double(Int((meters * ftPerM).rounded())))])
        case .metric:
            if meters >= kmThresholdM {
                let km = meters / 1000.0
                return T.string("units.distance.km", ["distance": .number((km * 10).rounded() / 10)])
            }
            return T.string("units.distance.m", ["distance": .number(Double(Int(meters.rounded())))])
        }
    }
}
