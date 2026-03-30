import SwiftUI

struct RoutePlanningView: View {
    @EnvironmentObject private var appModel: AppModel

    var body: some View {
        NavigationStack {
            Form {
                Section("Provider") {
                    Picker("Routing provider", selection: $appModel.selectedProviderID) {
                        ForEach(appModel.providerOptions) { provider in
                            HStack {
                                Text(provider.displayName)
                                if !provider.isAvailableInV1 {
                                    Text("Coming soon")
                                        .foregroundStyle(.secondary)
                                }
                            }
                            .tag(provider)
                        }
                    }
                }

                Section("Origin") {
                    CoordinateEditor(title: "Latitude", value: $appModel.routeRequest.origin.latitude)
                    CoordinateEditor(title: "Longitude", value: $appModel.routeRequest.origin.longitude)
                }

                Section("Destination") {
                    CoordinateEditor(title: "Latitude", value: $appModel.routeRequest.destination.latitude)
                    CoordinateEditor(title: "Longitude", value: $appModel.routeRequest.destination.longitude)
                }

                Section {
                    Button("Plan HSL route") {
                        Task { await appModel.planRoute() }
                    }
                    .disabled(!appModel.selectedProviderID.isAvailableInV1)
                }
            }
            .navigationTitle("Plan route")
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
