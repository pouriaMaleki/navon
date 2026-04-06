import SwiftUI

private struct ConnectionProvider: Identifiable {
    let id: String
    let title: String
    let subtitle: String
}

struct ConnectionsSettingsView: View {
    private let providers = [
        ConnectionProvider(id: "strava", title: "Strava", subtitle: "Inbound route integration planned"),
        ConnectionProvider(id: "garmin", title: "Garmin Connect", subtitle: "Inbound route integration planned"),
        ConnectionProvider(id: "komoot", title: "Komoot", subtitle: "Inbound route integration planned")
    ]

    var body: some View {
        List(providers) { provider in
            VStack(alignment: .leading, spacing: 4) {
                Text(provider.title)
                Text(provider.subtitle)
                    .font(.subheadline)
                    .foregroundStyle(.secondary)
            }
        }
        .navigationTitle("Connections")
    }
}
