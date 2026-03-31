import SwiftUI

struct ActiveRideView: View {
    @EnvironmentObject private var appModel: AppModel

    var body: some View {
        NavigationStack {
            List {
                Section("Active route") {
                    Text("Route id: \(appModel.activeSession.routeIdentifier ?? "None")")
                    Text("Revision: \(appModel.activeSession.routeRevision.map(String.init) ?? "0")")
                    Text("Destination: \(appModel.activeSession.destinationLabel)")
                    Text("Provider: \(appModel.activeSession.providerID.displayName)")
                    if let destination = appModel.activeSession.destinationCoordinate {
                        Text(String(format: "Destination lat/lon: %.5f, %.5f", destination.latitude, destination.longitude))
                    }
                }

                Section("Reroute") {
                    CoordinateEditor(title: "Rider latitude", value: $appModel.simulatedRiderLocation.latitude)
                    CoordinateEditor(title: "Rider longitude", value: $appModel.simulatedRiderLocation.longitude)
                    Text("Last reason: \(appModel.activeSession.lastRerouteReason ?? "No reroute yet")")
                    Text("Last timestamp: \(appModel.activeSession.lastRerouteTimestamp?.formatted() ?? "Never")")
                    Button("Request reroute from current rider location") {
                        Task { await appModel.handleRerouteRequest() }
                    }
                    .disabled(appModel.activeSession.destinationCoordinate == nil)
                }
            }
            .navigationTitle("Active ride")
        }
    }
}

private struct CoordinateEditor: View {
    let title: String
    @Binding var value: Double

    var body: some View {
        HStack {
            Text(title)
            Spacer()
            TextField(title, value: $value, format: .number)
                .multilineTextAlignment(.trailing)
                .keyboardType(.decimalPad)
                .frame(width: 140)
        }
    }
}
