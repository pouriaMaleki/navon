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
                    Text("Last reason: \(appModel.activeSession.lastRerouteReason ?? "No reroute yet")")
                    Text("Last timestamp: \(appModel.activeSession.lastRerouteTimestamp?.formatted() ?? "Never")")
                }
            }
            .navigationTitle("Active ride")
        }
    }
}
