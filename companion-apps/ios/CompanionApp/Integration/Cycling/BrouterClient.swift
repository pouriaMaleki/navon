import Foundation

/// Free OSM-based cycling routing. `timode=2` is required for turn instructions via `voicehints`.
enum BrouterProfile: String {
    case fastbike = "fastbike"
    case trekking = "trekking"
    case safety = "safety"
}

enum BrouterError: Error {
    case invalidURL
    case http(Int, String)
    case missingFeatures
}

struct BrouterClient {
    static let base = "https://brouter.de/brouter"

    static func fetch(
        profile: BrouterProfile,
        origin: CoordinatePoint,
        destination: CoordinatePoint
    ) async throws -> [String: Any] {
        let lonlats = String(
            format: "%.6f,%.6f|%.6f,%.6f",
            origin.longitude, origin.latitude,
            destination.longitude, destination.latitude
        )
        guard let escaped = lonlats.addingPercentEncoding(withAllowedCharacters: .urlQueryAllowed) else {
            throw BrouterError.invalidURL
        }
        guard let url = URL(string: "\(base)?lonlats=\(escaped)&profile=\(profile.rawValue)&alternativeidx=0&format=geojson&timode=2") else {
            throw BrouterError.invalidURL
        }
        let (data, response) = try await URLSession.shared.data(from: url)
        guard let http = response as? HTTPURLResponse else {
            throw BrouterError.http(-1, "Missing HTTP response")
        }
        guard (200..<300).contains(http.statusCode) else {
            throw BrouterError.http(http.statusCode, String(data: data, encoding: .utf8) ?? "")
        }
        guard let json = try JSONSerialization.jsonObject(with: data) as? [String: Any],
              let features = json["features"] as? [[String: Any]],
              let first = features.first else {
            throw BrouterError.missingFeatures
        }
        return first
    }
}
