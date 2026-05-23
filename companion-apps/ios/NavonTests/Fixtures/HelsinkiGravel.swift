import Foundation
@testable import Navon

/// Load the pre-baked helsinki-gravel fixture. The file lives under
/// `parity-fixtures/data/helsinki-gravel/stream.jsonl`. We resolve it relative
/// to this file so the test target does not need a resource bundle.
enum HelsinkiGravelFixture {
    struct Sample {
        let latitude: Double
        let longitude: Double
        let speedMps: Double
        let courseRad: Double
        let accuracyM: Double
        let timeOffsetMs: Int64
    }

    static func loadStream() -> [Sample] {
        let fixtureURL = URL(fileURLWithPath: #filePath)
            .deletingLastPathComponent()
            .deletingLastPathComponent()
            .deletingLastPathComponent()
            .deletingLastPathComponent()
            .deletingLastPathComponent()
            .appendingPathComponent("data/parity-fixtures/data/helsinki-gravel/stream.jsonl")
        guard let text = try? String(contentsOf: fixtureURL, encoding: .utf8) else {
            fatalError("helsinki-gravel fixture missing — run `cargo run -p xtask --bin gen-gps-fixtures`")
        }
        return text
            .split(separator: "\n", omittingEmptySubsequences: true)
            .compactMap { line -> Sample? in
                guard
                    let data = line.data(using: .utf8),
                    let json = try? JSONSerialization.jsonObject(with: data) as? [String: Any],
                    let lat = json["lat_deg"] as? Double,
                    let lon = json["lon_deg"] as? Double,
                    let speed = json["speed_mps"] as? Double,
                    let course = json["course_rad"] as? Double,
                    let accuracy = json["accuracy_m"] as? Double,
                    let t = json["t_ms"] as? Int
                else {
                    return nil
                }
                return Sample(
                    latitude: lat,
                    longitude: lon,
                    speedMps: speed,
                    courseRad: course,
                    accuracyM: accuracy,
                    timeOffsetMs: Int64(t)
                )
            }
    }
}
